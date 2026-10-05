mod completion;
mod git;
mod helper;
mod highlight;
mod history;
pub(crate) mod keys;
mod prompt;
pub(crate) mod theme;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::{Cmd, EditMode, Editor, EventHandler};

use crate::builtins;
use crate::shell::{self, Flow, KeyBinding, Shell};
use crate::signals;
use crate::sys;
use crate::table;

use completion::ShellNames;
use helper::ShellHelper;
use prompt::PromptState;

const USAGE: &str = "usage: grfsh [-l] [-i] [-c command [name [args...]]] [script [args...]]\n       grfsh --check script...";

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
    while let Some(option) = args.first() {
        match option.as_str() {
            "-l" | "--login" => login = true,
            "-i" => force_interactive = true,
            _ => break,
        }
        args.remove(0);
    }
    let mut shell = Shell::new();

    let status = match args.first().map(String::as_str) {
        Some("-h") | Some("--help") => {
            println!("{USAGE}");
            0
        }
        Some("--check") => crate::check::run(&args[1..]),
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
        Some(script) => run_script(&mut shell, script, &args[1..]),
        None => {
            if login {
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

fn interactive(shell: &mut Shell) {
    let mut editor = Editor::new().expect("failed to initialize editor");
    editor.set_helper(Some(ShellHelper::new()));
    let mut applied_bindings = 0;
    let mut vi_mode = false;
    let history = history_path();
    let _ = editor.load_history(&history);
    shell.history = editor
        .history()
        .iter()
        .map(|entry| entry.to_string())
        .collect();
    let mut warned_about_jobs = false;
    let mut duration = Duration::ZERO;
    let mut git_prompt = git::GitPrompt::default();
    let mut themes = theme::ThemeCache::default();

    loop {
        shell.reap_jobs();
        shell.run_hook("precmd", &[]);
        if shell.bindings_version != applied_bindings {
            apply_bindings(&mut editor, shell);
            applied_bindings = shell.bindings_version;
        }
        if shell.options.vi != vi_mode {
            vi_mode = shell.options.vi;
            editor.set_edit_mode(if vi_mode {
                EditMode::Vi
            } else {
                EditMode::Emacs
            });
        }
        if let Some(helper) = editor.helper_mut() {
            helper.update(completion_names(shell));
        }
        let directory = std::env::current_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let state = PromptState {
            git: git_prompt.info(&directory),
            status: shell.last_status,
            duration,
            jobs: shell.jobs.len(),
        };
        let columns = sys::terminal_width(1).unwrap_or(80);
        let current_theme = themes.current(&theme::theme_path(&config_dir()));
        let screen = match &current_theme {
            Some(current_theme) => {
                let context = theme::PromptContext {
                    status: state.status,
                    duration: state.duration,
                    jobs: state.jobs,
                    git: state.git.as_ref(),
                    directory: &directory,
                };
                let rendered = match current_theme.engine {
                    theme::Engine::Starship => theme::starship(&context, columns)
                        .unwrap_or_else(|| theme::render(current_theme, &context)),
                    theme::Engine::Native => theme::render(current_theme, &context),
                };
                theme::layout(rendered, columns, current_theme.transient)
            }
            None => theme::Screen::plain(prompt::build(&state)),
        };
        let screen = theme::Screen {
            prompt: crate::text::without_colors(screen.prompt),
            right: screen.right.map(|right| helper::RightPrompt {
                colored: crate::text::without_colors(right.colored),
                ..right
            }),
            transient: screen.transient.map(crate::text::without_colors),
        };
        if let Some(helper) = editor.helper_mut() {
            helper.set_right_prompt(screen.right);
        }
        let line = editor.readline(&screen.prompt);
        git_prompt.settle();
        if let (Ok(text), Some(transient)) = (&line, &screen.transient) {
            let highlighted = editor
                .helper()
                .map(|helper| highlight::highlight(text, |name| helper.is_command(name)))
                .unwrap_or_else(|| text.clone());
            theme::collapse(&screen.prompt, text, transient, &highlighted, columns);
        }
        match line {
            Ok(line) => {
                if line.trim().is_empty() {
                    duration = Duration::ZERO;
                    continue;
                }
                let line = match keys::expand_first_word(&line, &shell.abbreviations) {
                    Some(expanded) => {
                        eprintln!("{expanded}");
                        expanded
                    }
                    None => line,
                };
                let line = match history::expand(&line, &shell.history) {
                    Ok(Some(expanded)) => {
                        eprintln!("{expanded}");
                        expanded
                    }
                    Ok(None) => line,
                    Err(error) => {
                        shell::error(error);
                        shell.last_status = 1;
                        continue;
                    }
                };
                let _ = editor.add_history_entry(line.trim());
                shell.history.push(line.trim().to_string());
                shell.run_hook("preexec", std::slice::from_ref(&line));
                let started = Instant::now();
                let directory = std::env::var("PWD").unwrap_or_default();
                shell.run_source(&line);
                duration = started.elapsed();
                record_command(&line, &directory, duration, shell.last_status);
                if shell.flow == Flow::Interrupted {
                    shell.last_status = 128 + libc::SIGINT;
                }
                if shell.flow == Flow::Exit {
                    if shell.jobs.len() > 0 && !warned_about_jobs {
                        warned_about_jobs = true;
                        shell.flow = Flow::Normal;
                        shell::error("there are unfinished jobs; exit again to leave anyway");
                        continue;
                    }
                    break;
                }
                if shell.history_cleared {
                    let _ = editor.clear_history();
                    shell.history_cleared = false;
                }
                warned_about_jobs = false;
                shell.flow = Flow::Normal;
            }
            Err(ReadlineError::Interrupted) => {
                duration = Duration::ZERO;
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(error) => {
                if signals::any_pending() {
                    shell.handle_signals();
                    if shell.flow == Flow::Exit {
                        break;
                    }
                    continue;
                }
                shell::error(error);
                break;
            }
        }
    }

    let _ = editor.save_history(&history);
    sys::make_private(&history);
}

fn apply_bindings(
    editor: &mut Editor<ShellHelper, rustyline::history::DefaultHistory>,
    shell: &Shell,
) {
    for (spec, binding) in &shell.key_bindings {
        let Some(key) = keys::key(spec) else {
            continue;
        };
        let command = match binding {
            KeyBinding::Action(action) => keys::action(action),
            KeyBinding::Insert(text) => Some(Cmd::Insert(1, text.clone())),
        };
        if let Some(command) = command {
            editor.bind_sequence(key, EventHandler::Simple(command));
        }
    }
}

pub(crate) fn command_log_path() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("grfsh/commands.tsv")
}

fn record_command(line: &str, directory: &str, duration: Duration, status: i32) {
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
