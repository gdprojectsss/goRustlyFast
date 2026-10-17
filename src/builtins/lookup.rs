use std::io::Write;

use crate::shell::{self, Shell};
use crate::table;

use super::{deparse, is_builtin, names};

pub(crate) const DEFAULT_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "case", "esac", "for", "select", "while", "until", "do",
    "done", "in", "function", "time", "{", "}", "!", "[[", "]]", "coproc",
];

const SPECIAL_BUILTINS: &[&str] = &[
    ".", ":", "break", "continue", "eval", "exec", "exit", "export", "readonly", "return", "set",
    "shift", "source", "times", "trap", "unset",
];

pub(crate) fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(&name)
}

pub(crate) fn is_active(shell: &Shell, name: &str) -> bool {
    is_builtin(name)
        && !shell
            .disabled_builtins
            .iter()
            .any(|disabled| disabled == name)
}

fn aliases_enabled(shell: &Shell) -> bool {
    shell.options.expand_aliases || shell.interactive
}

pub(crate) fn path_matches(name: &str, path: &str, all: bool) -> Vec<String> {
    if name.contains('/') {
        return if crate::exec::is_executable(name) {
            vec![name.to_string()]
        } else {
            Vec::new()
        };
    }
    let mut found = Vec::new();
    for directory in path.split(':') {
        let candidate = if directory.is_empty() {
            format!("./{name}")
        } else {
            format!("{directory}/{name}")
        };
        if crate::exec::is_executable(&candidate) {
            found.push(candidate);
            if !all {
                break;
            }
        }
    }
    found
}

fn search_path(shell: &Shell) -> String {
    shell.get_variable("PATH").unwrap_or_default()
}

pub(crate) struct Prefixes {
    pub bypass_functions: bool,
    pub default_path: bool,
}

pub(crate) fn strip_prefixes(shell: &Shell, argv: &mut Vec<String>) -> Result<Prefixes, String> {
    let mut prefixes = Prefixes {
        bypass_functions: false,
        default_path: false,
    };
    loop {
        match argv.first().map(String::as_str) {
            Some("command") => {
                let mut options = 1;
                let mut describe = false;
                let mut default_path = false;
                while let Some(argument) = argv.get(options) {
                    if argument == "--" {
                        options += 1;
                        break;
                    }
                    if argument.len() < 2 || !argument.starts_with('-') {
                        break;
                    }
                    for letter in argument[1..].chars() {
                        match letter {
                            'p' => default_path = true,
                            'v' | 'V' => describe = true,
                            _ => return Ok(prefixes),
                        }
                    }
                    options += 1;
                }
                if describe {
                    return Ok(prefixes);
                }
                argv.drain(..options);
                prefixes.bypass_functions = true;
                prefixes.default_path |= default_path;
            }
            Some("builtin") => {
                let Some(name) = argv.get(1) else {
                    argv.clear();
                    return Ok(prefixes);
                };
                if !is_active(shell, name) {
                    return Err(format!("builtin: {name}: not a shell builtin"));
                }
                argv.remove(0);
                prefixes.bypass_functions = true;
                return Ok(prefixes);
            }
            _ => return Ok(prefixes),
        }
    }
}

#[derive(Default)]
struct TypeFlags {
    all: bool,
    skip_functions: bool,
    path_only: bool,
    force_path: bool,
    kind: bool,
}

fn parse_type_flags(args: &[String]) -> Result<(TypeFlags, &[String]), char> {
    let mut flags = TypeFlags::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        if argument.len() < 2 || !argument.starts_with('-') {
            break;
        }
        for letter in argument[1..].chars() {
            match letter {
                'a' => flags.all = true,
                'f' => flags.skip_functions = true,
                'p' => flags.path_only = true,
                'P' => flags.force_path = true,
                't' => flags.kind = true,
                other => return Err(other),
            }
        }
        index += 1;
    }
    Ok((flags, &args[index..]))
}

enum Found {
    Alias(String),
    Keyword,
    Function(String),
    Builtin,
    Table,
    File(String),
}

fn findings(shell: &Shell, name: &str, flags: &TypeFlags) -> Vec<Found> {
    let mut found = Vec::new();
    if !flags.force_path {
        if aliases_enabled(shell)
            && let Some(value) = shell.aliases.get(name)
        {
            found.push(Found::Alias(value.clone()));
        }
        if is_keyword(name) {
            found.push(Found::Keyword);
        }
        if !flags.skip_functions
            && let Some(body) = shell.functions.get(name)
        {
            found.push(Found::Function(deparse::function(name, body)));
        }
        if is_active(shell, name) {
            found.push(Found::Builtin);
        } else if table::is_command(name) {
            found.push(Found::Table);
        }
    }
    let lookup_path = flags.all || flags.force_path || found.is_empty();
    if lookup_path {
        found.extend(
            path_matches(name, &search_path(shell), flags.all)
                .into_iter()
                .map(Found::File),
        );
    }
    if !flags.all {
        found.truncate(1);
    }
    found
}

fn type_line(name: &str, found: &Found, flags: &TypeFlags) -> Option<String> {
    if flags.kind {
        return Some(
            match found {
                Found::Alias(_) => "alias",
                Found::Keyword => "keyword",
                Found::Function(_) => "function",
                Found::Builtin | Found::Table => "builtin",
                Found::File(_) => "file",
            }
            .to_string(),
        );
    }
    if flags.path_only || flags.force_path {
        return match found {
            Found::File(path) => Some(path.clone()),
            _ => None,
        };
    }
    Some(match found {
        Found::Alias(value) => format!("{name} is aliased to `{value}'"),
        Found::Keyword => format!("{name} is a shell keyword"),
        Found::Function(text) => format!("{name} is a function\n{text}"),
        Found::Builtin => format!("{name} is a shell builtin"),
        Found::Table => format!("{name} is a grfsh table command"),
        Found::File(path) => format!("{name} is {path}"),
    })
}

pub(super) fn type_command(shell: &Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let (flags, names) = match parse_type_flags(args) {
        Ok(parsed) => parsed,
        Err(letter) => {
            shell::error(format!("type: -{letter}: invalid option"));
            eprintln!("type: usage: type [-afptP] name [name ...]");
            return 2;
        }
    };
    let mut status = 0;
    for name in names {
        let found = findings(shell, name, &flags);
        if found.is_empty() {
            if !flags.kind && !flags.path_only && !flags.force_path {
                shell::error(format!("type: {name}: not found"));
            }
            status = 1;
            continue;
        }
        for item in &found {
            if let Some(line) = type_line(name, item, &flags) {
                let _ = writeln!(out, "{line}");
            }
        }
    }
    status
}

fn single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub(super) fn command(shell: &Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let mut verbose = false;
    let mut short = false;
    let mut default_path = false;
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        if argument.len() < 2 || !argument.starts_with('-') {
            break;
        }
        for letter in argument[1..].chars() {
            match letter {
                'p' => default_path = true,
                'v' => short = true,
                'V' => verbose = true,
                other => {
                    shell::error(format!("command: -{other}: invalid option"));
                    eprintln!("command: usage: command [-pVv] command [arg ...]");
                    return 2;
                }
            }
        }
        index += 1;
    }
    let names = &args[index..];
    if (!verbose && !short) || names.is_empty() {
        return 0;
    }
    let mut any_found = false;
    for name in names {
        let flags = TypeFlags::default();
        let mut found = findings(shell, name, &flags);
        if default_path && matches!(found.first(), Some(Found::File(_)) | None) {
            found = path_matches(name, DEFAULT_PATH, false)
                .into_iter()
                .map(Found::File)
                .collect();
        }
        let Some(item) = found.first() else {
            if verbose {
                shell::error(format!("command: {name}: not found"));
            }
            continue;
        };
        any_found = true;
        let line = if verbose {
            type_line(name, item, &flags).unwrap_or_default()
        } else {
            match item {
                Found::Alias(value) => format!("alias {name}={}", single_quote(value)),
                Found::File(path) => path.clone(),
                _ => name.clone(),
            }
        };
        let _ = writeln!(out, "{line}");
    }
    i32::from(!any_found)
}

pub(super) fn print_functions(
    shell: &Shell,
    names: &[String],
    names_only: bool,
    out: &mut dyn Write,
) -> i32 {
    let listed: Vec<String> = if names.is_empty() {
        let mut all: Vec<String> = shell.functions.keys().cloned().collect();
        all.sort();
        all
    } else {
        names.to_vec()
    };
    let mut status = 0;
    for name in listed {
        let Some(body) = shell.functions.get(&name) else {
            status = 1;
            continue;
        };
        let _ = match (names_only, names.is_empty()) {
            (true, true) => writeln!(out, "declare -f {name}"),
            (true, false) => writeln!(out, "{name}"),
            (false, _) => writeln!(out, "{}", deparse::function(&name, body)),
        };
    }
    status
}

pub(super) fn builtin(shell: &mut Shell, args: &[String]) -> i32 {
    let Some(name) = args.first() else {
        return 0;
    };
    if !is_active(shell, name) {
        shell::error(format!("builtin: {name}: not a shell builtin"));
        return 1;
    }
    super::run(shell, args)
}

fn sorted_builtins() -> Vec<&'static str> {
    let mut all: Vec<&'static str> = names().to_vec();
    all.sort();
    all
}

pub(super) fn enable(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let mut disable = false;
    let mut all = false;
    let mut special = false;
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        if argument.len() < 2 || !argument.starts_with('-') {
            break;
        }
        for letter in argument[1..].chars() {
            match letter {
                'n' => disable = true,
                'a' => all = true,
                's' => special = true,
                'p' => {}
                'f' | 'd' => {
                    shell::error(format!(
                        "enable: -{letter}: loadable builtins are not supported"
                    ));
                    return 1;
                }
                other => {
                    shell::error(format!("enable: -{other}: invalid option"));
                    eprintln!("enable: usage: enable [-a] [-dnps] [-f filename] [name ...]");
                    return 2;
                }
            }
        }
        index += 1;
    }
    let names = &args[index..];
    if names.is_empty() {
        for name in sorted_builtins() {
            let enabled = is_active(shell, name);
            let shown =
                (all || enabled != disable) && (!special || SPECIAL_BUILTINS.contains(&name));
            if shown {
                let flag = if enabled { "" } else { "-n " };
                let _ = writeln!(out, "enable {flag}{name}");
            }
        }
        return 0;
    }
    let mut status = 0;
    for name in names {
        if !is_builtin(name) {
            shell::error(format!("enable: {name}: not a shell builtin"));
            status = 1;
            continue;
        }
        shell.disabled_builtins.retain(|disabled| disabled != name);
        if disable {
            shell.disabled_builtins.push(name.clone());
        }
    }
    status
}

#[derive(Default)]
struct Completion {
    actions: Vec<String>,
    words: Option<String>,
    glob: Option<String>,
    filter: Option<String>,
    prefix: String,
    suffix: String,
}

fn parse_compgen(args: &[String]) -> Result<(Completion, Option<String>), String> {
    let mut completion = Completion::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        index += 1;
        if argument == "--" {
            break;
        }
        if argument.len() < 2 || !argument.starts_with('-') {
            index -= 1;
            break;
        }
        let letters: Vec<char> = argument[1..].chars().collect();
        for (position, letter) in letters.iter().enumerate() {
            let action = match letter {
                'a' => Some("alias"),
                'b' => Some("builtin"),
                'c' => Some("command"),
                'd' => Some("directory"),
                'e' => Some("export"),
                'f' => Some("file"),
                'k' => Some("keyword"),
                'u' => Some("user"),
                'v' => Some("variable"),
                _ => None,
            };
            if let Some(action) = action {
                completion.actions.push(action.to_string());
                continue;
            }
            let rest: String = letters[position + 1..].iter().collect();
            let value = if rest.is_empty() {
                let value = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| format!("-{letter}: option requires an argument"))?;
                index += 1;
                value
            } else {
                rest
            };
            match letter {
                'A' => completion.actions.push(value),
                'W' => completion.words = Some(value),
                'G' => completion.glob = Some(value),
                'X' => completion.filter = Some(value),
                'P' => completion.prefix = value,
                'S' => completion.suffix = value,
                'o' | 'F' | 'C' => {}
                other => return Err(format!("-{other}: invalid option")),
            }
            break;
        }
    }
    Ok((completion, args.get(index).cloned()))
}

fn sorted_unique(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items.dedup();
    items
}

fn variable_names(shell: &Shell) -> Vec<String> {
    let mut all: Vec<String> = shell
        .variables
        .keys()
        .chain(shell.arrays.keys())
        .cloned()
        .collect();
    all.extend(std::env::vars_os().filter_map(|(name, _)| name.into_string().ok()));
    sorted_unique(all)
}

fn file_candidates(word: &str, directories_only: bool) -> Vec<String> {
    let (directory, partial) = match word.rfind('/') {
        Some(slash) => (&word[..=slash], &word[slash + 1..]),
        None => ("", word),
    };
    let listing = if directory.is_empty() { "." } else { directory };
    let Ok(entries) = std::fs::read_dir(listing) else {
        return Vec::new();
    };
    let mut found: Vec<String> = Vec::new();
    if partial.starts_with('.') && !directories_only {
        found.extend([".", ".."].iter().map(|name| format!("{directory}{name}")));
    }
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(partial) {
            continue;
        }
        let path = format!("{directory}{name}");
        if directories_only && !std::path::Path::new(&path).is_dir() {
            continue;
        }
        found.push(path);
    }
    found.sort();
    found
}

fn action_candidates(shell: &mut Shell, action: &str, word: &str) -> Result<Vec<String>, String> {
    Ok(match action {
        "alias" => sorted_unique(shell.aliases.keys().cloned().collect()),
        "builtin" => sorted_builtins().into_iter().map(String::from).collect(),
        "keyword" => KEYWORDS.iter().map(|name| name.to_string()).collect(),
        "function" => sorted_unique(shell.functions.keys().cloned().collect()),
        "variable" => variable_names(shell),
        "export" => sorted_unique(
            std::env::vars_os()
                .filter_map(|(name, _)| name.into_string().ok())
                .collect(),
        ),
        "file" => file_candidates(word, false),
        "directory" => file_candidates(word, true),
        "user" => users(),
        "command" => {
            let mut all = sorted_unique(shell.aliases.keys().cloned().collect());
            all.extend(KEYWORDS.iter().map(|name| name.to_string()));
            all.extend(sorted_unique(shell.functions.keys().cloned().collect()));
            all.extend(sorted_builtins().into_iter().map(String::from));
            all.extend(table::names().iter().map(|name| name.to_string()));
            all.extend(shell.path_command_names().iter().cloned());
            all
        }
        other => return Err(format!("{other}: invalid action name")),
    })
}

fn users() -> Vec<String> {
    let contents = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    sorted_unique(
        contents
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split(':').next())
            .filter(|name| !name.is_empty())
            .map(String::from)
            .collect(),
    )
}

fn filtered(candidates: Vec<String>, filter: Option<&str>) -> Vec<String> {
    let Some(filter) = filter else {
        return candidates;
    };
    let (negate, pattern) = match filter.strip_prefix('!') {
        Some(pattern) => (true, pattern),
        None => (false, filter),
    };
    candidates
        .into_iter()
        .filter(|candidate| crate::expand::glob::matches_pattern(pattern, candidate) == negate)
        .collect()
}

pub(super) fn compgen(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let (completion, word) = match parse_compgen(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            shell::error(format!("compgen: {message}"));
            return 2;
        }
    };
    let word = word.unwrap_or_default();
    let mut candidates = Vec::new();
    for action in &completion.actions {
        match action_candidates(shell, action, &word) {
            Ok(found) => candidates.extend(found),
            Err(message) => {
                shell::error(format!("compgen: {message}"));
                return 2;
            }
        }
    }
    if let Some(pattern) = &completion.glob {
        candidates
            .extend(crate::expand::glob::expand(pattern, Default::default()).unwrap_or_default());
    }
    if let Some(words) = &completion.words {
        let ifs = shell
            .get_variable("IFS")
            .unwrap_or_else(|| crate::expand::DEFAULT_IFS.into());
        candidates.extend(crate::expand::split_ifs(words, &ifs, None));
    }
    let candidates: Vec<String> = candidates
        .into_iter()
        .filter(|candidate| candidate.starts_with(word.as_str()))
        .collect();
    let candidates = filtered(candidates, completion.filter.as_deref());
    for candidate in &candidates {
        let _ = writeln!(out, "{}{candidate}{}", completion.prefix, completion.suffix);
    }
    i32::from(candidates.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn command_prefixes_are_removed() {
        let shell = Shell::new();
        let mut argv = strings(&["command", "-p", "ls", "-l"]);
        let prefixes = strip_prefixes(&shell, &mut argv).unwrap();
        assert!(prefixes.bypass_functions && prefixes.default_path);
        assert_eq!(argv, strings(&["ls", "-l"]));
        let mut argv = strings(&["command", "-v", "ls"]);
        strip_prefixes(&shell, &mut argv).unwrap();
        assert_eq!(argv[0], "command");
        let mut argv = strings(&["builtin", "nosuch"]);
        assert!(strip_prefixes(&shell, &mut argv).is_err());
    }

    #[test]
    fn compgen_filters_by_prefix() {
        let mut shell = Shell::new();
        let mut out = Vec::new();
        let status = compgen(
            &mut shell,
            &strings(&["-W", "alpha beta alto", "al"]),
            &mut out,
        );
        assert_eq!(status, 0);
        assert_eq!(String::from_utf8(out).unwrap(), "alpha\nalto\n");
    }

    #[test]
    fn keywords_are_described() {
        let shell = Shell::new();
        let mut out = Vec::new();
        type_command(&shell, &strings(&["-t", "if", "cd"]), &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), "keyword\nbuiltin\n");
    }
}
