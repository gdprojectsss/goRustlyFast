use std::io::Write;

use crate::shell::{self, Shell};
use crate::syntax::parser::is_name;
use crate::variables::{Array, Attributes, Key, array_assignment, quote_value};

use super::parse_count;

#[derive(Default)]
struct UnsetMode {
    functions: bool,
    variables: bool,
    reference: bool,
}

fn parse_unset_options(args: &[String]) -> Result<(UnsetMode, &[String]), String> {
    let mut mode = UnsetMode::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        let Some(letters) = argument
            .strip_prefix('-')
            .filter(|letters| !letters.is_empty())
        else {
            break;
        };
        for letter in letters.chars() {
            match letter {
                'f' => mode.functions = true,
                'v' => mode.variables = true,
                'n' => mode.reference = true,
                other => return Err(format!("unset: -{other}: invalid option")),
            }
        }
        index += 1;
    }
    Ok((mode, &args[index..]))
}

pub(super) fn unset(shell: &mut Shell, args: &[String]) -> i32 {
    let (mode, names) = match parse_unset_options(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            shell::error(message);
            return 2;
        }
    };
    let mut status = 0;
    for name in names {
        if mode.functions {
            remove_function(shell, name);
            continue;
        }
        if !unset_one(shell, name, &mode) {
            status = 1;
        }
    }
    status
}

fn remove_function(shell: &mut Shell, name: &str) {
    shell.functions.remove(name);
    shell.function_sources.remove(name);
    shell.unexport_function(name);
}

fn variable_exists(shell: &Shell, name: &str) -> bool {
    shell.get_variable(name).is_some()
        || shell.arrays.contains_key(name)
        || shell.attributes.contains_key(name)
}

fn unset_one(shell: &mut Shell, name: &str, mode: &UnsetMode) -> bool {
    let valid =
        is_name(name) || shell::split_subscript(name).is_some_and(|(array, _)| is_name(array));
    if !valid && !mode.variables {
        remove_function(shell, name);
        return true;
    }
    let Some((array, key)) = shell::split_subscript(name) else {
        if !is_name(name) {
            shell::error(format!("unset: '{name}': not a valid identifier"));
            return false;
        }
        if mode.reference {
            return shell.unset_name(name);
        }
        if !mode.variables && !variable_exists(shell, name) && shell.functions.contains_key(name) {
            remove_function(shell, name);
            return true;
        }
        return shell.unset_variable(name);
    };
    if !is_name(array) {
        shell::error(format!("unset: '{name}': not a valid identifier"));
        return false;
    }
    let array = shell.resolve_name(array);
    if shell.is_readonly(&array) {
        shell::error(format!("unset: {array}: cannot unset: readonly variable"));
        return false;
    }
    if key == "@" || key == "*" {
        if let Some(values) = shell.arrays.get_mut(&array) {
            *values = if values.is_associative() {
                Array::associative()
            } else {
                Array::indexed(Vec::new())
            };
        }
        return true;
    }
    let key = if shell.is_associative(&array) {
        Key::Name(key.to_string())
    } else {
        match crate::expand::evaluate_arithmetic(shell, key) {
            Ok(index) => Key::Index(index),
            Err(_) => {
                shell::error(format!("unset: {name}: bad array subscript"));
                return false;
            }
        }
    };
    match shell.arrays.get_mut(&array) {
        Some(values) => values.remove(&key),
        None if key == Key::Index(0) => {
            shell.unset_variable(&array);
        }
        None => {}
    }
    true
}

#[derive(Default, Clone, Copy)]
struct AttributeFlags {
    indexed: bool,
    associative: bool,
    readonly: bool,
    export: bool,
    integer: bool,
    lower: bool,
    upper: bool,
    nameref: bool,
}

impl AttributeFlags {
    fn any(&self) -> bool {
        self.indexed
            || self.associative
            || self.readonly
            || self.export
            || self.integer
            || self.lower
            || self.upper
            || self.nameref
    }

    fn changes_value(&self) -> bool {
        self.indexed || self.associative || self.integer || self.lower || self.upper
    }

    fn matches(&self, shell: &Shell, name: &str) -> bool {
        let attributes = shell.attributes.get(name).copied().unwrap_or_default();
        let array = shell.arrays.get(name);
        (!self.indexed || array.is_some_and(|array| !array.is_associative()))
            && (!self.associative || array.is_some_and(Array::is_associative))
            && (!self.readonly || attributes.readonly)
            && (!self.export || shell.is_exported(name))
            && (!self.integer || attributes.integer)
            && (!self.lower || attributes.lower)
            && (!self.upper || attributes.upper)
            && (!self.nameref || attributes.nameref)
    }
}

#[derive(Default)]
struct Flags {
    add: AttributeFlags,
    remove: AttributeFlags,
    print: bool,
    global: bool,
    functions: bool,
    function_names: bool,
}

fn parse_flags(command: &str, args: &[String]) -> Result<(Flags, Vec<String>), String> {
    let mut flags = Flags::default();
    let mut names = Vec::new();
    let mut options_done = false;
    for argument in args {
        let enable = argument.starts_with('-');
        if options_done || !(enable || argument.starts_with('+')) || argument.len() == 1 {
            options_done = true;
            names.push(argument.clone());
            continue;
        }
        if argument == "--" {
            options_done = true;
            continue;
        }
        for flag in argument[1..].chars() {
            let target = if enable {
                &mut flags.add
            } else {
                &mut flags.remove
            };
            match flag {
                'a' => target.indexed = true,
                'A' => target.associative = true,
                'r' => target.readonly = true,
                'x' => target.export = true,
                'i' => target.integer = true,
                'l' => target.lower = true,
                'u' => target.upper = true,
                'n' if command == "export" => flags.remove.export = enable,
                'n' => target.nameref = true,
                'p' => flags.print = true,
                'g' => flags.global = true,
                'f' => flags.functions = true,
                'F' => {
                    flags.functions = true;
                    flags.function_names = true;
                }
                other => return Err(format!("-{other}: invalid option")),
            }
        }
    }
    Ok((flags, names))
}

pub(crate) fn attribute_letters(shell: &Shell, name: &str) -> String {
    let attributes = shell.attributes.get(name).copied().unwrap_or_default();
    let mut letters = String::new();
    if let Some(array) = shell.arrays.get(name) {
        letters.push(if array.is_associative() { 'A' } else { 'a' });
    }
    for (enabled, letter) in [
        (attributes.integer, 'i'),
        (attributes.nameref, 'n'),
        (attributes.readonly, 'r'),
        (shell.is_exported(name), 'x'),
        (attributes.lower, 'l'),
        (attributes.upper, 'u'),
    ] {
        if enabled {
            letters.push(letter);
        }
    }
    letters
}

pub(crate) fn describe(shell: &Shell, name: &str) -> Option<String> {
    let mut letters = attribute_letters(shell, name);
    if letters.is_empty() {
        letters.push('-');
    }
    let attributes = shell.attributes.get(name).copied().unwrap_or_default();
    if let Some(array) = shell.arrays.get(name) {
        if attributes.invisible {
            return Some(format!("declare -{letters} {name}"));
        }
        return Some(format!(
            "declare -{letters} {name}={}",
            array_assignment(array)
        ));
    }
    let value = if attributes.nameref {
        shell.variables.get(name).cloned()
    } else if shell.variables.contains_key(name) || !attributes.invisible {
        shell
            .variables
            .get(name)
            .cloned()
            .or_else(|| crate::bytes::env_var(name))
    } else {
        None
    };
    match value {
        Some(value) => Some(format!("declare -{letters} {name}={}", quote_value(&value))),
        None if shell.attributes.contains_key(name) => Some(format!("declare -{letters} {name}")),
        None => None,
    }
}

fn exported_names(shell: &Shell) -> Vec<String> {
    std::env::vars_os()
        .map(|(name, _)| crate::bytes::from_os(&name))
        .filter(|name| is_name(name) && !shell.variables.contains_key(name))
        .chain(
            shell
                .attributes
                .iter()
                .filter(|(_, attributes)| attributes.exported)
                .map(|(name, _)| name.clone()),
        )
        .collect()
}

fn listed_names(shell: &Shell, command: &str, filter: &AttributeFlags) -> Vec<String> {
    let mut names: Vec<String> = match command {
        "export" => exported_names(shell),
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
            .chain(shell.attributes.keys())
            .cloned()
            .chain(exported_names(shell))
            .filter(|name| filter.matches(shell, name))
            .collect(),
    };
    names.sort();
    names.dedup();
    names
}

fn print_declarations(
    shell: &Shell,
    command: &str,
    flags: &Flags,
    names: Vec<String>,
    out: &mut dyn Write,
) -> i32 {
    if !names.is_empty() && command == "export" {
        return 0;
    }
    let listed = if names.is_empty() {
        listed_names(shell, command, &flags.add)
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
    status
}

fn export_functions(shell: &mut Shell, flags: &Flags, names: &[String]) -> i32 {
    let mut status = 0;
    for name in names {
        if flags.remove.export {
            shell.unexport_function(name);
        } else if !shell.export_function(name) {
            shell::error(format!("export: {name}: not a function"));
            status = 1;
        }
    }
    status
}

fn missing_functions(shell: &Shell, command: &str, names: &[String]) -> i32 {
    let mut status = 0;
    for name in names
        .iter()
        .filter(|name| !shell.functions.contains_key(*name))
    {
        shell::error(format!("{command}: {name}: not a function"));
        status = 1;
    }
    status
}

fn apply_value_attributes(shell: &mut Shell, name: &str, flags: &Flags) {
    let attributes = shell.attributes.entry(name.to_string()).or_default();
    if flags.add.integer {
        attributes.integer = true;
    }
    if flags.add.lower {
        attributes.lower = true;
        attributes.upper = false;
    }
    if flags.add.upper {
        attributes.upper = true;
        attributes.lower = false;
    }
    attributes.integer &= !flags.remove.integer;
    attributes.lower &= !flags.remove.lower;
    attributes.upper &= !flags.remove.upper;
    if flags.remove.nameref {
        attributes.nameref = false;
    }
    if *attributes == Attributes::default() {
        shell.attributes.remove(name);
    }
}

fn has_value(shell: &Shell, name: &str) -> bool {
    shell.variables.contains_key(name)
        || shell.arrays.contains_key(name)
        || std::env::var_os(name).is_some()
}

fn prepare_target(
    shell: &mut Shell,
    command: &str,
    name: &str,
    flags: &Flags,
    is_local: bool,
    assigned: bool,
) -> Result<(), String> {
    let first_local = is_local && {
        let saved = shell.snapshot(name);
        shell
            .locals
            .last_mut()
            .is_some_and(|frame| match frame.entry(name.to_string()) {
                std::collections::hash_map::Entry::Occupied(_) => false,
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(saved);
                    true
                }
            })
    };
    if flags.add.nameref || flags.remove.nameref {
        if shell.is_readonly(name) {
            return Err(format!("{command}: {name}: readonly variable"));
        }
        if first_local {
            shell.variables.remove(name);
            shell.arrays.remove(name);
            shell.attributes.remove(name);
        }
        apply_value_attributes(shell, name, flags);
        if flags.add.nameref {
            shell
                .attributes
                .entry(name.to_string())
                .or_default()
                .nameref = true;
            if !assigned && !shell.variables.contains_key(name) {
                shell
                    .attributes
                    .entry(name.to_string())
                    .or_default()
                    .invisible = true;
            }
        }
        return Ok(());
    }
    let target = shell.resolve_name(name);
    let name = target.as_str();
    if shell.is_readonly(name) && (assigned || flags.add.changes_value()) {
        return Err(format!("{command}: {name}: readonly variable"));
    }
    if first_local && !flags.add.indexed && !flags.add.associative {
        shell.variables.remove(name);
        shell.arrays.remove(name);
        let inherited = shell.attributes.remove(name).unwrap_or_default();
        if !assigned {
            let attributes = shell.attributes.entry(name.to_string()).or_default();
            attributes.invisible = true;
            attributes.exported = inherited.exported;
        }
    }
    apply_value_attributes(shell, name, flags);
    if flags.add.associative && !shell.is_associative(name) {
        if shell.arrays.contains_key(name) {
            return Err(format!(
                "{command}: {name}: cannot convert indexed to associative array"
            ));
        }
        shell.arrays.insert(name.to_string(), Array::associative());
        shell.variables.remove(name);
        if !assigned {
            shell
                .attributes
                .entry(name.to_string())
                .or_default()
                .invisible = true;
        }
    } else if flags.add.indexed && !shell.arrays.contains_key(name) {
        let existing: Vec<String> = shell.variables.remove(name).into_iter().collect();
        if !assigned && existing.is_empty() {
            shell
                .attributes
                .entry(name.to_string())
                .or_default()
                .invisible = true;
        }
        shell
            .arrays
            .insert(name.to_string(), Array::indexed(existing));
    } else if !assigned && !has_value(shell, name) && !first_local {
        shell
            .attributes
            .entry(name.to_string())
            .or_default()
            .invisible = true;
    }
    Ok(())
}

fn finish_target(shell: &mut Shell, command: &str, name: &str, flags: &Flags) {
    let target = if flags.add.nameref {
        name.to_string()
    } else {
        shell.resolve_name(name)
    };
    if flags.add.export || command == "export" {
        shell.export_variable(&target, None);
    }
    if flags.remove.export {
        shell.unexport_variable(&target);
    }
    if flags.add.readonly || command == "readonly" {
        shell.attributes.entry(target).or_default().readonly = true;
    }
}

fn assign_reference(
    shell: &mut Shell,
    command: &str,
    name: &str,
    value: &str,
) -> Result<(), String> {
    if !value.is_empty() && !is_name(value) && shell::split_subscript(value).is_none() {
        return Err(format!(
            "{command}: '{value}': invalid variable name for name reference"
        ));
    }
    if value == name {
        return Err(format!(
            "{command}: {name}: nameref variable self references not allowed"
        ));
    }
    shell.variables.insert(name.to_string(), value.to_string());
    if let Some(attributes) = shell.attributes.get_mut(name) {
        attributes.invisible = false;
    }
    Ok(())
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

    if command == "export" && flags.functions && !names.is_empty() {
        return export_functions(shell, &flags, &names);
    }
    if command == "readonly" && flags.functions && !names.is_empty() {
        return missing_functions(shell, command, &names);
    }
    if flags.functions {
        return super::lookup::print_functions(shell, &names, flags.function_names, out);
    }
    let listing = names.is_empty() && declarations.is_empty();
    if flags.print || (listing && (!flags.add.any() || command != "declare")) {
        return print_declarations(shell, command, &flags, names, out);
    }
    if listing {
        return print_declarations(shell, command, &flags, Vec::new(), out);
    }

    let mut status = 0;
    let mut failed: Vec<String> = Vec::new();
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
            status = 1;
            failed.push(name.clone());
            continue;
        }
        let assigned = declarations
            .iter()
            .any(|declaration| declaration.name() == name);
        if let Err(message) = prepare_target(shell, command, name, &flags, is_local, assigned) {
            shell::error(message);
            status = 1;
            failed.push(name.clone());
        }
    }

    for declaration in declarations {
        let name = declaration.name().to_string();
        if failed.contains(&name) {
            continue;
        }
        let result = match &declaration {
            crate::variables::Evaluated::Scalar {
                key: None, value, ..
            } if shell.is_reference(&name) && (flags.add.nameref || !has_target(shell, &name)) => {
                assign_reference(shell, command, &name, value)
            }
            _ => shell
                .assign(declaration)
                .map_err(|message| format!("{command}: {message}")),
        };
        if let Err(message) = result {
            shell::error(message);
            status = 1;
            failed.push(name);
        }
    }

    for name in &targets {
        if !failed.contains(name) {
            finish_target(shell, command, name, &flags);
        }
    }
    status
}

fn has_target(shell: &Shell, name: &str) -> bool {
    shell
        .variables
        .get(name)
        .is_some_and(|target| !target.is_empty())
}

pub(super) fn shift(shell: &mut Shell, args: &[String]) -> i32 {
    let count = match parse_count(shell, "shift", args, 1) {
        Ok(count) if count >= 0 => count as usize,
        Ok(count) => {
            shell::error(format!("shift: {count}: shift count out of range"));
            return 1;
        }
        Err(code) => return code,
    };
    if count > shell.positional.len() {
        return 1;
    }
    shell.positional.drain(..count);
    0
}
