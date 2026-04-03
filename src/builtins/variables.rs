use std::io::Write;

use crate::shell::{self, Shell};
use crate::syntax::parser::is_name;

use super::parse_count;

pub(super) fn export(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    if args.is_empty() || args[0] == "-p" {
        let mut variables: Vec<_> = std::env::vars().collect();
        variables.sort();
        for (key, value) in variables {
            let _ = writeln!(
                out,
                "export {key}=\"{}\"",
                value.replace('\\', "\\\\").replace('"', "\\\"")
            );
        }
        return 0;
    }
    let mut status = 0;
    for argument in args {
        let (name, value) = match argument.split_once('=') {
            Some((key, value)) => (key, Some(value)),
            None => (argument.as_str(), None),
        };
        if !is_name(name) {
            shell::error(format!("export: '{argument}': not a valid identifier"));
            status = 1;
            continue;
        }
        shell.export_variable(name, value);
    }
    status
}

pub(super) fn unset(shell: &mut Shell, args: &[String]) -> i32 {
    let functions = args.first().is_some_and(|argument| argument == "-f");
    for name in args.iter().filter(|argument| !argument.starts_with('-')) {
        if functions {
            shell.functions.remove(name);
        } else {
            shell.unset_variable(name);
        }
    }
    0
}

pub(super) fn local(shell: &mut Shell, args: &[String]) -> i32 {
    if shell.locals.is_empty() {
        shell::error("local: can only be used in a function");
        return 1;
    }
    for argument in args {
        let (name, value) = argument.split_once('=').unwrap_or((argument.as_str(), ""));
        if !is_name(name) {
            shell::error(format!("local: '{argument}': not a valid identifier"));
            return 1;
        }
        let previous = shell.variables.get(name).cloned();
        if let Some(frame) = shell.locals.last_mut() {
            frame.entry(name.to_string()).or_insert(previous);
        }
        shell.variables.insert(name.to_string(), value.to_string());
    }
    0
}

pub(super) fn shift(shell: &mut Shell, args: &[String]) -> i32 {
    let count = match parse_count("shift", args, 1) {
        Ok(count) => count.max(0) as usize,
        Err(code) => return code,
    };
    if count > shell.positional.len() {
        return 1;
    }
    shell.positional.drain(..count);
    0
}

pub(super) fn alias(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let mut sorted: Vec<_> = shell.aliases.iter().collect();
        sorted.sort_by_key(|(key, _)| *key);
        for (name, value) in sorted {
            let _ = writeln!(out, "alias {name}='{value}'");
        }
        return 0;
    }

    for argument in args {
        match argument.split_once('=') {
            Some((name, value)) => {
                shell.aliases.insert(name.into(), value.into());
            }
            None => match shell.aliases.get(argument.as_str()) {
                Some(value) => {
                    let _ = writeln!(out, "alias {argument}='{value}'");
                }
                None => {
                    shell::error(format!("alias: {argument}: not found"));
                    return 1;
                }
            },
        }
    }
    0
}

pub(super) fn unalias(shell: &mut Shell, args: &[String]) -> i32 {
    if args.first().is_some_and(|argument| argument == "-a") {
        shell.aliases.clear();
        return 0;
    }
    for argument in args {
        shell.aliases.remove(argument.as_str());
    }
    0
}
