mod completion;
mod foreign;
mod git;
mod helper;
mod highlight;
mod history;
pub(crate) mod keys;
mod prompt;
mod session;
pub(crate) mod theme;

use std::path::PathBuf;
use std::time::Duration;

use crate::builtins;
use crate::cli::{self, Invocation, Mode};
use crate::shell::{self, Flow, Shell};
use crate::sys;
use crate::table;

use completion::ShellNames;
pub(crate) use foreign::startup_file_hint;
pub(crate) use history::fc;
use session::interactive;

fn install_panic_handler() {
    std::panic::set_hook(Box::new(|info| {
        sys::restore_saved_terminal();
        let location = info
            .location()
            .map(|location| format!(" at {}:{}", location.file(), location.line()))
            .unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".into());
        eprintln!(
            "\ngrfsh hit an internal error{location}: {message}\nPlease report it at https://github.com/gdprojectsss/goRustlyFast/issues with the command that triggered it."
        );
    }));
}

pub fn run() -> i32 {
    install_panic_handler();
    sys::unblock_signals();
    let mut args: Vec<String> = std::env::args_os()
        .map(|argument| crate::bytes::from_os(&argument))
        .collect();
    let invoked_as = if args.is_empty() {
        String::new()
    } else {
        args.remove(0)
    };
    let mut invocation = match cli::parse(&args) {
        Ok(invocation) => invocation,
        Err(error) => {
            shell::error(&error.message);
            if error.show_usage {
                eprintln!("{}", cli::USAGE);
            }
            return 2;
        }
    };
    match invocation.mode {
        Mode::Help => {
            println!("{}", cli::USAGE);
            return 0;
        }
        Mode::Version => {
            println!("grfsh {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        _ => {}
    }
    invocation.login |= invoked_as.starts_with('-');
    invocation.agent |= crate::agent::requested();
    crate::startup::initialize_environment();
    let mut shell = Shell::new();
    if !invoked_as.is_empty() {
        shell.arg0 = invoked_as;
    }
    if invocation.agent {
        crate::agent::prepare_environment();
        shell.agent = true;
    }
    let capture = begin_capture(&invocation);
    let status = run_invocation(&mut shell, &invocation);
    shell.last_status = status;
    shell.run_exit_trap();
    if let Some(capture) = capture {
        report_capture(capture, shell.last_status, invocation.json);
    }
    shell.last_status
}

fn begin_capture(invocation: &Invocation) -> Option<crate::agent::Capture> {
    let captured = match invocation.mode {
        Mode::Command(_) | Mode::Script(_) | Mode::Check(_) | Mode::Explain(_) => true,
        Mode::Standard => !invocation.interactive,
        _ => false,
    };
    if !invocation.agent || !captured {
        return None;
    }
    match crate::agent::Capture::begin(invocation.json) {
        Ok(started) => Some(started),
        Err(error) => {
            shell::error(format!("--agent: {}", sys::error_message(&error)));
            None
        }
    }
}

fn report_capture(capture: crate::agent::Capture, status: i32, json: bool) {
    let outcome = capture.finish();
    if json {
        let directory = std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let result = crate::agent::result_json(&outcome, status, &directory);
        println!("{}", result.to_text());
    } else {
        crate::agent::print_plain(&outcome);
    }
}

fn run_invocation(shell: &mut Shell, invocation: &Invocation) -> i32 {
    match &invocation.mode {
        Mode::Check(paths) => crate::check::run(paths),
        Mode::Explain(args) => crate::agent::explain_arguments(shell, args),
        Mode::Serve(args) => match args.first() {
            Some(path) => crate::agent::serve(shell, path),
            None => {
                shell::error("--serve: expected a socket path");
                2
            }
        },
        Mode::Client(args) => match args.first() {
            Some(path) => crate::agent::client(path, &args[1..], invocation.json),
            None => {
                shell::error("--client: expected a socket path");
                2
            }
        },
        _ => run_shell(shell, invocation),
    }
}

fn run_shell(shell: &mut Shell, invocation: &Invocation) -> i32 {
    let reads_input = invocation.mode == Mode::Standard;
    let interactive_shell = invocation.interactive || (reads_input && sys::isatty(0));
    if let Err(status) = apply_options(shell, invocation) {
        return status;
    }
    if interactive_shell {
        if sys::isatty(0) {
            shell.shell_pgid = sys::init_job_control();
            sys::remember_terminal();
        }
        shell.enable_interactive_defaults();
    }
    shell.options.login_shell = invocation.login;
    load_startup_files(shell, invocation, interactive_shell);
    if shell.flow == Flow::Exit {
        return shell.last_status;
    }
    shell.positional = invocation.positional.clone();
    shell.reading_stdin = !matches!(invocation.mode, Mode::Command(_) | Mode::Script(_));
    match &invocation.mode {
        Mode::Command(source) => run_command_string(shell, source, invocation),
        Mode::Script(path) => run_script_operand(shell, path, invocation),
        _ if interactive_shell => {
            welcome_once();
            load_plugins(shell);
            interactive(shell);
            shell.last_status
        }
        _ if invocation.no_execute => {
            let source = crate::bytes::decode(&sys::read_all(0));
            crate::startup::check_syntax(&source)
        }
        _ => shell.run_lines(|| sys::read_line(0)),
    }
}

fn apply_options(shell: &mut Shell, invocation: &Invocation) -> Result<(), i32> {
    let mut commands = Vec::new();
    if !invocation.set_arguments.is_empty() {
        let mut set = vec!["set".to_string()];
        set.extend(invocation.set_arguments.iter().cloned());
        commands.push(set);
    }
    commands.extend(invocation.shopt_arguments.iter().cloned());
    for command in commands {
        if builtins::run(shell, &command) != 0 {
            return Err(2);
        }
    }
    Ok(())
}

fn load_startup_files(shell: &mut Shell, invocation: &Invocation, interactive: bool) {
    if invocation.login && !invocation.no_profile {
        add_system_paths();
        load_file(shell, &profile_path());
    }
    if interactive && !invocation.no_rc && shell.flow != Flow::Exit {
        let rc = invocation
            .rc_file
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(rc_path);
        load_file(shell, &rc);
    }
}

fn run_command_string(shell: &mut Shell, source: &str, invocation: &Invocation) -> i32 {
    if let Some(name) = &invocation.name {
        shell.arg0 = name.clone();
    }
    shell.command_string = true;
    shell::set_location(Some(invocation.name.as_deref().unwrap_or_default()), 0);
    if invocation.no_execute {
        return crate::startup::check_syntax(source);
    }
    let mut lines = source.lines().map(String::from);
    shell.run_lines(|| lines.next())
}

fn run_script_operand(shell: &mut Shell, path: &str, invocation: &Invocation) -> i32 {
    let exists = std::path::Path::new(path).exists();
    if !exists && (builtins::is_builtin(path) || table::is_command(path)) {
        let words: Vec<String> = std::iter::once(path)
            .chain(invocation.positional.iter().map(String::as_str))
            .map(|word| format!("'{}'", word.replace('\'', "'\\''")))
            .collect();
        shell.run_source(&words.join(" "));
        return shell.last_status;
    }
    if invocation.no_execute {
        return check_script(path);
    }
    run_script(shell, path, &invocation.positional)
}

fn check_script(path: &str) -> i32 {
    match crate::bytes::read_file(path) {
        Ok(contents) => {
            shell::set_location(Some(path), 0);
            crate::startup::check_syntax(&contents)
        }
        Err(error) => {
            shell::error(format!("{path}: {}", sys::error_message(&error)));
            127
        }
    }
}

fn run_script(shell: &mut Shell, path: &str, args: &[String]) -> i32 {
    let mut reader = match crate::bytes::LineReader::open(path) {
        Ok(reader) => reader,
        Err(error) => {
            shell::error(format!("{path}: {}", sys::error_message(&error)));
            return 127;
        }
    };
    shell.arg0 = path.to_string();
    shell.source_stack.push(path.to_string());
    shell.positional = args.to_vec();
    shell::set_location(Some(path), 0);
    shell.run_lines(|| reader.next_line())
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| crate::bytes::from_os(&home))
        .or_else(sys::current_user_home)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn history_path() -> PathBuf {
    home_dir().join(".grfsh_history")
}

pub(crate) fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("grfsh")
}

fn first_existing(candidates: [PathBuf; 2]) -> PathBuf {
    let [preferred, fallback] = candidates;
    if preferred.exists() {
        preferred
    } else {
        fallback
    }
}

fn rc_path() -> PathBuf {
    first_existing([config_dir().join("grfshrc"), home_dir().join(".grfshrc")])
}

fn system_path(listed: &[String], current: &str) -> String {
    let mut entries: Vec<&str> = Vec::new();
    for entry in listed.iter().map(String::as_str).chain(current.split(':')) {
        if !entry.is_empty() && !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    entries.join(":")
}

fn add_system_paths() {
    let mut files = vec![PathBuf::from("/etc/paths")];
    if let Ok(directory) = std::fs::read_dir("/etc/paths.d") {
        let mut extra: Vec<PathBuf> = directory.flatten().map(|entry| entry.path()).collect();
        extra.sort();
        files.extend(extra);
    }
    let listed: Vec<String> = files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| {
            text.lines()
                .map(|line| line.trim().to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    if listed.is_empty() {
        return;
    }
    let current = std::env::var_os("PATH")
        .map(|path| crate::bytes::from_os(&path))
        .unwrap_or_default();
    unsafe { std::env::set_var("PATH", crate::bytes::to_os(&system_path(&listed, &current))) };
}

fn profile_path() -> PathBuf {
    first_existing([
        config_dir().join("profile"),
        home_dir().join(".grfsh_profile"),
    ])
}

fn load_file(shell: &mut Shell, path: &std::path::Path) {
    if !path.exists() {
        return;
    }
    if let Err(problem) = crate::trust::check(path) {
        warn_once(format!("not loading {problem}"));
        return;
    }
    let Ok(contents) = std::fs::read(path).map(|bytes| crate::bytes::decode(&bytes)) else {
        return;
    };
    let mut lines = contents.lines().map(String::from);
    shell.run_lines(|| lines.next());
}

fn warn_once(message: String) {
    static WARNED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let Ok(mut warned) = WARNED.lock() else {
        return;
    };
    if !warned.contains(&message) {
        shell::error(&message);
        warned.push(message);
    }
}

pub(crate) fn command_log_path() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("grfsh/commands.tsv")
}

pub(crate) fn log_command(line: &str, directory: &str, duration: Duration, status: i32) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = command_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)
    else {
        return;
    };
    let escape = |text: &str| {
        text.replace('\\', "\\\\")
            .replace('\t', "\\t")
            .replace('\n', "\\n")
    };
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
        .saturating_sub(duration.as_secs());
    let _ = writeln!(
        file,
        "{started}\t{}\t{status}\t{}\t{}",
        duration.as_millis(),
        escape(directory),
        escape(line.trim())
    );
}

fn welcome_once() {
    let directory = config_dir();
    let marker = directory.join(".welcome-shown");
    if marker.exists() || theme::theme_path(&directory).exists() || rc_path().exists() {
        return;
    }
    println!(
        "Welcome to grfsh {}. Run configure-prompt to pick a prompt style, or help for the list of builtins.",
        env!("CARGO_PKG_VERSION")
    );
    if std::fs::create_dir_all(&directory).is_ok() {
        let _ = std::fs::write(marker, "");
    }
}

fn load_plugins(shell: &mut Shell) {
    let Ok(entries) = std::fs::read_dir(config_dir().join("plugins")) else {
        return;
    };
    let mut plugins: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path().join("init.grfsh"))
        .filter(|path| path.is_file())
        .collect();
    plugins.sort();
    for plugin in plugins {
        load_file(shell, &plugin);
    }
}

fn completion_names(shell: &mut Shell) -> ShellNames {
    let mut commands = shell.path_command_names().to_vec();
    commands.extend(builtins::names().iter().map(|name| name.to_string()));
    commands.extend(table::names().iter().map(|name| name.to_string()));
    commands.extend(shell.functions.keys().cloned());
    commands.extend(shell.aliases.keys().cloned());

    let mut variables: Vec<String> = std::env::vars_os()
        .map(|(name, _)| crate::bytes::from_os(&name))
        .collect();
    variables.extend(shell.variables.keys().cloned());
    variables.sort();
    variables.dedup();

    ShellNames {
        commands,
        variables,
        aliases: shell.aliases.keys().cloned().collect(),
        jobs: shell
            .jobs
            .iter()
            .map(|job| format!("%{}", job.id))
            .collect(),
        specs: shell.completion_specs.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_paths_come_first_without_duplicates() {
        let listed = vec![
            "/usr/local/bin".to_string(),
            "/usr/bin".to_string(),
            "/opt/homebrew/bin".to_string(),
        ];
        assert_eq!(
            system_path(&listed, "/usr/bin:/home/me/bin::/usr/local/bin"),
            "/usr/local/bin:/usr/bin:/opt/homebrew/bin:/home/me/bin"
        );
    }
}
