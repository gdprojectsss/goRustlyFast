mod completion;
mod git;
mod helper;
mod highlight;
mod history;
mod prompt;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rustyline::Editor;
use rustyline::error::ReadlineError;

use crate::builtins;
use crate::shell::{self, Flow, Shell};
use crate::sys;
use crate::table;

use completion::ShellNames;
use helper::ShellHelper;
use prompt::PromptState;

const USAGE: &str = "usage: grfsh [-l] [-i] [-c command [name [args...]]] [script [args...]]";

pub fn run() -> i32 {
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
            shell::set_location(Some(&shell.arg0.clone()), 0);
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
                shell.interactive = true;
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

fn config_dir() -> PathBuf {
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
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    let mut lines = contents.lines().map(String::from);
    shell.run_lines(|| lines.next());
}

fn interactive(shell: &mut Shell) {
    let mut editor = Editor::new().expect("failed to initialize editor");
    editor.set_helper(Some(ShellHelper::new()));
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

    loop {
        shell.reap_jobs();
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
        let line = editor.readline(&prompt::build(&state));
        git_prompt.settle();
        match line {
            Ok(line) => {
                if line.trim().is_empty() {
                    duration = Duration::ZERO;
                    continue;
                }
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
                let started = Instant::now();
                shell.run_source(&line);
                duration = started.elapsed();
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
                shell::error(error);
                break;
            }
        }
    }

    let _ = editor.save_history(&history);
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
    }
}
