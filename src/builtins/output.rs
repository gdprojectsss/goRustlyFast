use std::io::Write;

use crate::shell::{self, Shell};
use crate::table;

use super::is_builtin;

pub(super) fn echo(args: &[String], out: &mut dyn Write) -> i32 {
    let mut newline = true;
    let mut escapes = false;
    let mut rest = args;
    while let Some(flag) = rest.first().filter(|argument| {
        argument.len() > 1
            && argument.starts_with('-')
            && argument[1..]
                .chars()
                .all(|character| "neE".contains(character))
    }) {
        for character in flag[1..].chars() {
            match character {
                'n' => newline = false,
                'e' => escapes = true,
                _ => escapes = false,
            }
        }
        rest = &rest[1..];
    }
    let mut text = rest.join(" ");
    if escapes {
        text = unescape(&text);
    }
    let _ = write!(out, "{text}");
    if newline {
        let _ = writeln!(out);
    }
    0
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('e') => out.push('\x1b'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

pub(super) fn describe(shell: &Shell, args: &[String], out: &mut dyn Write, short: bool) -> i32 {
    let mut status = 0;
    for name in args {
        let line = if let Some(value) = shell.aliases.get(name) {
            if short {
                format!("alias {name}='{value}'")
            } else {
                format!("{name} is aliased to '{value}'")
            }
        } else if shell.functions.contains_key(name) {
            if short {
                name.clone()
            } else {
                format!("{name} is a function")
            }
        } else if is_builtin(name) {
            if short {
                name.clone()
            } else {
                format!("{name} is a shell builtin")
            }
        } else if table::is_command(name) {
            if short {
                name.clone()
            } else {
                format!("{name} is a grfsh table command")
            }
        } else if let Some(path) = shell.resolve(name) {
            if short {
                path
            } else {
                format!("{name} is {path}")
            }
        } else {
            if !short {
                shell::error(format!("type: {name}: not found"));
            }
            status = 1;
            continue;
        };
        let _ = writeln!(out, "{line}");
    }
    status
}

pub(super) fn history(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let count = match args.first().map(String::as_str) {
        None => shell.history.len(),
        Some("-t") | Some("--table") => {
            let contents =
                std::fs::read_to_string(crate::repl::command_log_path()).unwrap_or_default();
            let _ = out.flush();
            return crate::table::command_log(shell, &contents);
        }
        Some("-c") => {
            shell.history.clear();
            shell.history_cleared = true;
            return 0;
        }
        Some(number) => match number.parse::<usize>() {
            Ok(count) => count,
            Err(_) => {
                shell::error(format!("history: {number}: numeric argument required"));
                return 2;
            }
        },
    };
    let start = shell.history.len().saturating_sub(count);
    for (index, line) in shell.history.iter().enumerate().skip(start) {
        let _ = writeln!(out, "{:>5}  {line}", index + 1);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_escapes() {
        assert_eq!(unescape("a\\tb\\n"), "a\tb\n");
        assert_eq!(unescape("keep\\q"), "keep\\q");
    }
}
