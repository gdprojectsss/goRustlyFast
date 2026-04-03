mod completer;
mod prompt;

use std::path::PathBuf;

use rustyline::Editor;
use rustyline::error::ReadlineError;

use crate::shell::{self, Flow, Shell};
use crate::syntax::parser;
use crate::sys;

use completer::ShellHelper;

pub fn run() -> i32 {
    let mut shell = Shell::new();
    load_rc(&mut shell);
    if shell.flow != Flow::Exit {
        if sys::isatty(0) {
            shell.shell_pgid = sys::init_job_control();
            shell.interactive = true;
            interactive(&mut shell);
        } else {
            run_lines(&mut shell, || sys::read_line(0));
        }
    }
    shell.last_status
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn history_path() -> PathBuf {
    home_dir().join(".grfsh_history")
}

fn rc_path() -> PathBuf {
    home_dir().join(".grfshrc")
}

fn load_rc(shell: &mut Shell) {
    let Ok(contents) = std::fs::read_to_string(rc_path()) else {
        return;
    };
    let mut lines = contents.lines().map(String::from);
    run_lines(shell, || lines.next());
}

fn run_lines(shell: &mut Shell, mut next_line: impl FnMut() -> Option<String>) {
    let mut buffer = String::new();
    while let Some(line) = next_line() {
        buffer.push_str(&line);
        if !buffer.ends_with('\n') {
            buffer.push('\n');
        }
        match parser::parse_with_aliases(&buffer, &shell.aliases) {
            Err(error) if error.incomplete => continue,
            Err(error) => {
                shell::error(error);
                shell.last_status = 2;
            }
            Ok(list) => {
                shell.run_list(&list);
                shell.reap_jobs();
            }
        }
        buffer.clear();
        if shell.flow == Flow::Exit {
            return;
        }
        shell.flow = Flow::Normal;
    }
    if !buffer.trim().is_empty() {
        shell.run_source(&buffer);
    }
}

fn interactive(shell: &mut Shell) {
    let mut editor = Editor::new().expect("failed to initialize editor");
    editor.set_helper(Some(ShellHelper::new()));
    let history = history_path();
    let _ = editor.load_history(&history);
    let mut warned_about_jobs = false;

    loop {
        shell.reap_jobs();
        match editor.readline(&prompt::build()) {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(line.trim());
                shell.run_source(&line);
                if shell.flow == Flow::Exit {
                    if shell.jobs.len() > 0 && !warned_about_jobs {
                        warned_about_jobs = true;
                        shell.flow = Flow::Normal;
                        shell::error("there are unfinished jobs; exit again to leave anyway");
                        continue;
                    }
                    break;
                }
                warned_about_jobs = false;
                shell.flow = Flow::Normal;
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(error) => {
                shell::error(error);
                break;
            }
        }
    }

    let _ = editor.save_history(&history);
}
