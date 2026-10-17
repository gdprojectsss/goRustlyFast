use std::io::Write;

use crate::shell::{self, Shell};
use crate::syntax::parser::is_name;
use crate::variables::{Array, Key};

use super::parse_count;

pub(super) fn unset(shell: &mut Shell, args: &[String]) -> i32 {
    let functions = args.first().is_some_and(|argument| argument == "-f");
    let mut status = 0;
    for name in args.iter().filter(|argument| !argument.starts_with('-')) {
        if functions {
            shell.functions.remove(name);
            continue;
        }
        let Some((array, rest)) = name.split_once('[') else {
            shell.unset_variable(name);
            continue;
        };
        let Some(key) = rest.strip_suffix(']') else {
            shell::error(format!("unset: {name}: not a valid identifier"));
            status = 1;
            continue;
        };
        if shell.is_readonly(array) {
            shell::error(format!("unset: {array}: readonly variable"));
            status = 1;
            continue;
        }
        let key = if shell.is_associative(array) {
            Key::Name(key.to_string())
        } else {
            match key.trim().parse() {
                Ok(index) => Key::Index(index),
                Err(_) => {
                    shell::error(format!("unset: {name}: bad array subscript"));
                    status = 1;
                    continue;
                }
            }
        };
        if let Some(values) = shell.arrays.get_mut(array) {
            values.remove(&key);
        }
    }
    status
}

#[derive(Default)]
struct Flags {
    indexed: bool,
    associative: bool,
    readonly: bool,
    export: bool,
    integer: bool,
    lower: bool,
    upper: bool,
    print: bool,
    global: bool,
    functions: bool,
}

fn parse_flags(command: &str, args: &[String]) -> Result<(Flags, Vec<String>), String> {
    let mut flags = Flags::default();
    let mut names = Vec::new();
    let mut options_done = false;
    for argument in args {
        if options_done || !argument.starts_with('-') || argument == "-" {
            names.push(argument.clone());
            continue;
        }
        if argument == "--" {
            options_done = true;
            continue;
        }
        for flag in argument[1..].chars() {
            match flag {
                'a' => flags.indexed = true,
                'A' => flags.associative = true,
                'r' => flags.readonly = true,
                'x' => flags.export = true,
                'i' => flags.integer = true,
                'l' => flags.lower = true,
                'u' => flags.upper = true,
                'p' => flags.print = true,
                'g' => flags.global = true,
                'f' | 'F' => flags.functions = true,
                'n' if command == "export" => {}
                other => return Err(format!("-{other}: invalid option")),
            }
        }
    }
    Ok((flags, names))
}

fn quote(value: &str) -> String {
    if crate::escapes::needs_ansi_quote(value) {
        return crate::escapes::ansi_quote(value);
    }
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    )
}

pub(crate) fn attribute_letters(shell: &Shell, name: &str) -> String {
    let attributes = shell.attributes.get(name).copied().unwrap_or_default();
    let mut letters = String::new();
    if let Some(array) = shell.arrays.get(name) {
        letters.push(if array.is_associative() { 'A' } else { 'a' });
    }
    let exported = std::env::var_os(name).is_some() && !shell.variables.contains_key(name);
    for (enabled, letter) in [
        (attributes.integer, 'i'),
        (attributes.lower, 'l'),
        (attributes.upper, 'u'),
        (attributes.readonly, 'r'),
        (exported, 'x'),
    ] {
        if enabled {
            letters.push(letter);
        }
    }
    letters
}

fn describe(shell: &Shell, name: &str) -> Option<String> {
    let mut letters = attribute_letters(shell, name);
    if letters.is_empty() {
        letters.push('-');
    }
    if let Some(array) = shell.arrays.get(name) {
        let elements: Vec<String> = array
            .keys()
            .iter()
            .zip(array.values())
            .map(|(key, value)| format!("[{key}]={}", quote(&value)))
            .collect();
        return Some(format!(
            "declare -{letters} {name}=({})",
            elements.join(" ")
        ));
    }
    let value = shell.get_variable(name)?;
    Some(format!("declare -{letters} {name}={}", quote(&value)))
}

fn listed_names(shell: &Shell, command: &str) -> Vec<String> {
    let mut names: Vec<String> = match command {
        "export" => std::env::vars_os()
            .map(|(name, _)| crate::bytes::from_os(&name))
            .collect(),
        "readonly" => shell
            .attributes
            .iter()
            .filter(|(_, attributes)| attributes.readonly)
            .map(|(name, _)| name.clone())
            .collect(),
        _ => shell
            .variables
            .keys()
            .chain(shell.arrays.keys())
            .cloned()
            .collect(),
    };
    names.sort();
    names.dedup();
    names
}

pub(super) fn declare(
    shell: &mut Shell,
    command: &str,
    args: &[String],
    out: &mut dyn Write,
) -> i32 {
    let declarations = std::mem::take(&mut shell.pending_declarations);
    let (flags, names) = match parse_flags(command, args) {
        Ok(parsed) => parsed,
        Err(message) => {
            shell::error(format!("{command}: {message}"));
            return 2;
        }
    };
    if command == "local" && shell.locals.is_empty() {
        shell::error("local: can only be used in a function");
        return 1;
    }
    let is_local = command == "local"
        || (matches!(command, "declare" | "typeset") && !flags.global && !shell.locals.is_empty());

    if flags.functions {
        let mut functions: Vec<&String> = shell.functions.keys().collect();
        functions.sort();
        for name in functions {
            let _ = writeln!(out, "declare -f {name}");
        }
        return 0;
    }
    if flags.print || (names.is_empty() && declarations.is_empty()) {
        let listed = if names.is_empty() {
            listed_names(shell, command)
        } else {
            names
        };
        let mut status = 0;
        for name in listed {
            match describe(shell, &name) {
                Some(line) => {
                    let _ = writeln!(out, "{line}");
                }
                None => {
                    shell::error(format!("{command}: {name}: not found"));
                    status = 1;
                }
            }
        }
        return status;
    }

    let targets: Vec<String> = names
        .iter()
        .cloned()
        .chain(
            declarations
                .iter()
                .map(|declaration| declaration.name().to_string()),
        )
        .collect();
    for name in &targets {
        if !is_name(name) {
            shell::error(format!("{command}: '{name}': not a valid identifier"));
            return 1;
        }
        let assigned = declarations
            .iter()
            .any(|declaration| declaration.name() == name);
        if shell.is_readonly(name) && (assigned || flags.indexed || flags.associative) {
            shell::error(format!("{command}: {name}: readonly variable"));
            return 1;
        }
        if is_local {
            let saved = shell.snapshot(name);
            if let Some(frame) = shell.locals.last_mut() {
                frame.entry(name.clone()).or_insert(saved);
            }
        }
        if flags.integer || flags.lower || flags.upper {
            let attributes = shell.attributes.entry(name.clone()).or_default();
            attributes.integer |= flags.integer;
            attributes.lower |= flags.lower;
            attributes.upper |= flags.upper;
        }
        if flags.associative && !shell.is_associative(name) {
            shell.arrays.insert(name.clone(), Array::associative());
            shell.variables.remove(name);
        } else if flags.indexed && !shell.arrays.contains_key(name) {
            let existing = shell.variables.remove(name).into_iter().collect();
            shell.arrays.insert(name.clone(), Array::indexed(existing));
        } else if is_local && !assigned && !shell.arrays.contains_key(name) {
            shell.variables.insert(name.clone(), String::new());
        }
    }

    for declaration in declarations {
        if let Err(message) = shell.assign(declaration) {
            shell::error(format!("{command}: {message}"));
            return 1;
        }
    }

    let mut status = 0;
    for name in &targets {
        if flags.export || command == "export" {
            if shell.arrays.contains_key(name) {
                shell::error(format!("{command}: {name}: arrays cannot be exported"));
                status = 1;
            } else {
                shell.export_variable(name, None);
            }
        }
        if flags.readonly || command == "readonly" {
            shell.attributes.entry(name.clone()).or_default().readonly = true;
        }
    }
    status
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
