use std::io::Write;

use crate::shell::{self, Flow, Shell};
use crate::syntax::parser::is_name;
use crate::sys;

pub(super) fn source(shell: &mut Shell, args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        shell::error("source: filename argument required");
        return 2;
    };
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            shell::error(format!("source: {path}: {}", sys::error_message(&error)));
            return 1;
        }
    };

    let saved_positional =
        (args.len() > 1).then(|| std::mem::replace(&mut shell.positional, args[1..].to_vec()));
    shell.source_depth += 1;
    let mut lines = contents.lines().map(String::from);
    let status = shell.run_lines(|| lines.next());
    shell.source_depth -= 1;
    if shell.flow == Flow::Return {
        shell.flow = Flow::Normal;
    }
    if let Some(positional) = saved_positional {
        shell.positional = positional;
    }
    status
}

pub(super) fn read(shell: &mut Shell, args: &[String]) -> i32 {
    let mut raw = false;
    let mut prompt = None;
    let mut names = Vec::new();
    let mut iter = args.iter();
    while let Some(argument) = iter.next() {
        match argument.as_str() {
            "-r" => raw = true,
            "-p" => prompt = iter.next().cloned(),
            name if is_name(name) => names.push(name.to_string()),
            other => {
                shell::error(format!("read: '{other}': not a valid identifier"));
                return 2;
            }
        }
    }
    if names.is_empty() {
        names.push("REPLY".into());
    }
    if let Some(prompt) = prompt {
        eprint!("{prompt}");
        let _ = std::io::stderr().flush();
    }

    let mut line = String::new();
    let mut reached_end = true;
    while let Some(chunk) = sys::read_line(0) {
        let complete = chunk.ends_with('\n');
        let chunk = chunk.strip_suffix('\n').unwrap_or(&chunk);
        if !raw && let Some(continued) = chunk.strip_suffix('\\') {
            line.push_str(continued);
            continue;
        }
        line.push_str(chunk);
        reached_end = !complete;
        break;
    }
    if !raw {
        line = remove_backslashes(&line);
    }

    assign_fields(shell, &names, &line);
    if reached_end && line.is_empty() { 1 } else { 0 }
}

fn remove_backslashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
        } else {
            out.push(character);
        }
    }
    out
}

fn assign_fields(shell: &mut Shell, names: &[String], line: &str) {
    let mut rest = line.trim_start_matches([' ', '\t']);
    for (index, name) in names.iter().enumerate() {
        if index + 1 == names.len() {
            shell.set_variable(name, rest.trim_end_matches([' ', '\t']));
            break;
        }
        let end = rest.find([' ', '\t']).unwrap_or(rest.len());
        shell.set_variable(name, &rest[..end]);
        rest = rest[end..].trim_start_matches([' ', '\t']);
    }
}

pub(super) fn set(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    match args.first().map(String::as_str) {
        None => {
            let mut variables: Vec<_> = shell.variables.iter().collect();
            variables.sort();
            for (name, value) in variables {
                let _ = writeln!(out, "{name}='{value}'");
            }
            0
        }
        Some("--") => {
            shell.positional = args[1..].to_vec();
            0
        }
        Some(other) => {
            shell::error(format!("set: {other}: unsupported option"));
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_split_with_remainder_in_last_name() {
        let mut shell = Shell::new();
        let names = vec!["first".to_string(), "rest".to_string()];
        assign_fields(&mut shell, &names, "  one two  three ");
        assert_eq!(shell.get_variable("first").as_deref(), Some("one"));
        assert_eq!(shell.get_variable("rest").as_deref(), Some("two  three"));
    }

    #[test]
    fn missing_fields_are_empty() {
        let mut shell = Shell::new();
        let names = vec!["first".to_string(), "second".to_string()];
        assign_fields(&mut shell, &names, "only");
        assert_eq!(shell.get_variable("second").as_deref(), Some(""));
    }

    #[test]
    fn backslashes_escape_the_next_character() {
        assert_eq!(remove_backslashes("a\\ b\\\\c"), "a b\\c");
    }
}
