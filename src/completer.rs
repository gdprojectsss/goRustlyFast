use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Context;

pub struct ShellHelper {
    file_completer: FilenameCompleter,
    commands: Vec<String>,
}

impl ShellHelper {
    pub fn new() -> Self {
        ShellHelper {
            file_completer: FilenameCompleter::new(),
            commands: collect_commands(),
        }
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before = &line[..pos];

        let cmd_start = before
            .rfind(['|', ';', '&'])
            .map(|i| i + 1)
            .unwrap_or(0);

        let fragment = before[cmd_start..].trim_start();

        if fragment.contains(' ') {
            return self.file_completer.complete(line, pos, ctx);
        }

        let word_start = pos - fragment.len();
        let matches = self
            .commands
            .iter()
            .filter(|c| c.starts_with(fragment))
            .map(|c| Pair {
                display: c.clone(),
                replacement: c.clone(),
            })
            .collect();

        Ok((word_start, matches))
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}
impl rustyline::Helper for ShellHelper {}

fn collect_commands() -> Vec<String> {
    let mut commands = BTreeSet::new();

    for b in ["cd", "exit", "export", "echo", "alias", "unalias"] {
        commands.insert(b.into());
    }

    let Ok(path) = std::env::var("PATH") else {
        return commands.into_iter().collect();
    };

    for dir in path.split(':') {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_file()
                && meta.permissions().mode() & 0o111 != 0
                && let Ok(name) = entry.file_name().into_string()
            {
                commands.insert(name);
            }
        }
    }

    commands.into_iter().collect()
}
