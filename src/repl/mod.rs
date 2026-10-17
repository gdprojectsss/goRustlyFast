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
use crate::shell::{self, Flow, Shell};
use crate::sys;
use crate::table;

use completion::ShellNames;
pub(crate) use foreign::startup_file_hint;
pub(crate) use history::fc;
use session::interactive;

const USAGE: &str = "usage: grfsh [-l] [-i] [--agent] [--json] [-c command [name [args...]]] [script [args...]]\n       grfsh --check script...\n       grfsh --explain command | --explain -f script\n       grfsh --serve socket | --client socket [-t seconds] command";

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
    let mut args: Vec<String> = std::env::args().collect();
    let invoked_as = if args.is_empty() {
        String::new()
    } else {
        args.remove(0)
    };
    let mut login = invoked_as.starts_with('-');
    let mut force_interactive = false;
    let mut agent = crate::agent::requested();
    let mut json = false;
    while let Some(option) = args.first() {
        match option.as_str() {
            "-l" | "--login" => login = true,
            "-i" => force_interactive = true,
            "--agent" => agent = true,
            "--json" => {
                agent = true;
                json = true;
            }
            _ => break,
        }
        args.remove(0);
    }
    let mut shell = Shell::new();
    let runs_code = match args.first().map(String::as_str) {
        Some("-c") => args.len() > 1,
        Some("--serve") => {
            agent = true;
            false
        }
        Some(option) => !option.starts_with('-') || option == "-",
        None => false,
    };
    let mut capture = None;
    if agent {
        crate::agent::prepare_environment();
        shell.agent = true;
        if runs_code {
            match crate::agent::Capture::begin(json) {
                Ok(started) => capture = Some(started),
                Err(error) => shell::error(format!("--agent: {}", sys::error_message(&error))),
            }
        }
    }

    let status = match args.first().map(String::as_str) {
        Some("-h") | Some("--help") => {
            println!("{USAGE}");
            0
        }
        Some("--check") => crate::check::run(&args[1..]),
        Some("--serve") => match args.get(1) {
            Some(path) => crate::agent::serve(&mut shell, path),
            None => {
                shell::error("--serve: expected a socket path");
                2
            }
        },
        Some("--client") => match args.get(1) {
            Some(path) => crate::agent::client(path, &args[2..], json),
            None => {
                shell::error("--client: expected a socket path");
                2
            }
        },
        Some("--explain") => crate::agent::explain_arguments(&mut shell, &args[1..]),
        Some("-V") | Some("--version") => {
            println!("grfsh {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("-c") => {
            let Some(command) = args.get(1) else {
                shell::error("-c: option requires an argument");
                return 2;
            };
            if let Some(name) = args.get(2) {
                shell.arg0 = name.clone();
            }
            shell.positional = args.iter().skip(3).cloned().collect();
            let name = args.get(2).cloned().unwrap_or_default();
            shell::set_location(Some(&name), 0);
            shell.run_source(command);
            shell.last_status
        }
        Some(option) if option.starts_with('-') && option != "-" => {
            shell::error(format!("{option}: unknown option\n{USAGE}"));
            2
        }
        Some(name)
            if !std::path::Path::new(name).exists()
                && (builtins::is_builtin(name) || table::is_command(name)) =>
        {
            let words: Vec<String> = args
                .iter()
                .map(|word| format!("'{}'", word.replace('\'', "'\\''")))
                .collect();
            shell.run_source(&words.join(" "));
            shell.last_status
        }
        Some(script) => run_script(&mut shell, script, &args[1..]),
        None => {
            if login {
                add_system_paths();
                load_file(&mut shell, &profile_path());
            }
            if shell.flow != Flow::Exit {
                load_file(&mut shell, &rc_path());
            }
            if shell.flow == Flow::Exit {
                return shell.last_status;
            }
            if force_interactive || sys::isatty(0) {
                shell.shell_pgid = sys::init_job_control();
                sys::remember_terminal();
                shell.interactive = true;
                welcome_once();
                load_plugins(&mut shell);
                interactive(&mut shell);
            } else {
                shell.run_lines(|| sys::read_line(0));
            }
            shell.last_status
        }
    };
    shell.last_status = status;
    shell.run_exit_trap();
    if let Some(capture) = capture {
        let outcome = capture.finish();
        if json {
            let directory = std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            let result = crate::agent::result_json(&outcome, shell.last_status, &directory);
            println!("{}", result.to_text());
        } else {
            crate::agent::print_plain(&outcome);
        }
    }
    shell.last_status
}

fn run_script(shell: &mut Shell, path: &str, args: &[String]) -> i32 {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            shell::error(format!("{path}: {}", sys::error_message(&error)));
            return 127;
        }
    };
    shell.arg0 = path.to_string();
    shell.source_stack.push(path.to_string());
    shell.positional = args.to_vec();
    shell::set_location(Some(path), 0);
    let mut lines = contents.lines().map(String::from);
    shell.run_lines(|| lines.next())
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
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
    let current = std::env::var("PATH").unwrap_or_default();
    unsafe { std::env::set_var("PATH", system_path(&listed, &current)) };
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
        shell::error(format!("not loading {problem}"));
        return;
    }
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    let mut lines = contents.lines().map(String::from);
    shell.run_lines(|| lines.next());
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

    let mut variables: Vec<String> = std::env::vars().map(|(name, _)| name).collect();
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
