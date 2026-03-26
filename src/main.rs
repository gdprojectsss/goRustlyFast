mod builtins;
mod completer;
mod executor;
mod parser;
mod prompt;
mod shell;

use completer::ShellHelper;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use shell::Shell;
use std::path::PathBuf;

fn history_path() -> PathBuf {
    home_dir().join(".grfsh_history")
}

fn rc_path() -> PathBuf {
    home_dir().join(".grfshrc")
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn load_rc(shell: &mut Shell) {
    let Ok(contents) = std::fs::read_to_string(rc_path()) else {
        return;
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parser::parse(&shell.expand_aliases(line)) {
            Ok(cmdline) => {
                executor::execute(&cmdline, &mut shell.aliases);
            }
            Err(e) => shell::error(format!("rc: {e}")),
        }
    }
}

fn main() {
    let mut shell = Shell::new();
    load_rc(&mut shell);

    let mut rl = Editor::new().expect("failed to initialize editor");
    rl.set_helper(Some(ShellHelper::new()));
    let history = history_path();
    let _ = rl.load_history(&history);

    loop {
        match rl.readline(&prompt::build()) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);
                match parser::parse(&shell.expand_aliases(line)) {
                    Ok(cmdline) => {
                        executor::execute(&cmdline, &mut shell.aliases);
                    }
                    Err(e) => shell::error(e),
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                shell::error(e);
                break;
            }
        }
    }

    let _ = rl.save_history(&history);
}
