use std::io::Write;

use crate::expand::{DEFAULT_IFS, split_ifs};
use crate::shell::{self, Flow, Shell, ShellOptions};
use crate::syntax::parser::is_name;
use crate::sys;
use crate::variables::Array;

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
    shell.source_stack.push(path.clone());
    let line = shell.current_line;
    let mut lines = contents.lines().map(String::from);
    let status = shell.run_lines(|| lines.next());
    shell.current_line = line;
    shell.source_stack.pop();
    shell.source_depth -= 1;
    if shell.flow == Flow::Return {
        shell.flow = Flow::Normal;
    }
    if let Some(positional) = saved_positional {
        shell.positional = positional;
    }
    status
}

struct ReadOptions {
    raw: bool,
    silent: bool,
    prompt: Option<String>,
    array: Option<String>,
    count: Option<usize>,
    timeout: Option<f64>,
    delimiter: u8,
}

fn parse_read_options(args: &[String]) -> Result<(ReadOptions, Vec<String>), String> {
    let mut options = ReadOptions {
        raw: false,
        silent: false,
        prompt: None,
        array: None,
        count: None,
        timeout: None,
        delimiter: b'\n',
    };
    let mut names = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        index += 1;
        if !argument.starts_with('-') || argument.len() < 2 {
            if !is_name(argument) {
                return Err(format!("'{argument}': not a valid identifier"));
            }
            names.push(argument.clone());
            continue;
        }
        let flags: Vec<char> = argument[1..].chars().collect();
        for (position, flag) in flags.iter().enumerate() {
            let takes_value = matches!(flag, 'p' | 'a' | 'n' | 't' | 'd');
            if !takes_value {
                match flag {
                    'r' => options.raw = true,
                    's' => options.silent = true,
                    other => return Err(format!("-{other}: invalid option")),
                }
                continue;
            }
            let attached: String = flags[position + 1..].iter().collect();
            let value = if attached.is_empty() {
                let value = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| format!("-{flag}: option requires an argument"))?;
                index += 1;
                value
            } else {
                attached
            };
            match flag {
                'p' => options.prompt = Some(value),
                'a' => options.array = Some(value),
                'n' => {
                    options.count = Some(
                        value
                            .parse()
                            .map_err(|_| format!("{value}: invalid number"))?,
                    )
                }
                't' => {
                    options.timeout = Some(
                        value
                            .parse()
                            .map_err(|_| format!("{value}: invalid timeout"))?,
                    )
                }
                _ => options.delimiter = value.bytes().next().unwrap_or(0),
            }
            break;
        }
    }
    Ok((options, names))
}

enum Ending {
    Delimiter,
    EndOfInput,
    TimedOut,
    Count,
}

fn read_input(options: &ReadOptions) -> (Vec<u8>, Ending) {
    let deadline = options
        .timeout
        .map(|seconds| std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds));
    let mut bytes = Vec::new();
    loop {
        if options.count.is_some_and(|count| bytes.len() >= count) {
            return (bytes, Ending::Count);
        }
        if let Some(deadline) = deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if !sys::wait_readable(0, remaining) {
                return (bytes, Ending::TimedOut);
            }
        }
        let mut byte = [0u8; 1];
        match sys::read(0, &mut byte) {
            Ok(1) => {}
            _ => return (bytes, Ending::EndOfInput),
        }
        if byte[0] == options.delimiter {
            return (bytes, Ending::Delimiter);
        }
        if !options.raw && byte[0] == b'\\' {
            let mut next = [0u8; 1];
            match sys::read(0, &mut next) {
                Ok(1) if next[0] == b'\n' => continue,
                Ok(1) => {
                    bytes.push(b'\\');
                    bytes.push(next[0]);
                }
                _ => return (bytes, Ending::EndOfInput),
            }
            continue;
        }
        bytes.push(byte[0]);
    }
}

pub(super) fn read(shell: &mut Shell, args: &[String]) -> i32 {
    let (options, mut names) = match parse_read_options(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            shell::error(format!("read: {message}"));
            return 2;
        }
    };
    if names.is_empty() && options.array.is_none() {
        names.push("REPLY".into());
    }
    if let Some(prompt) = &options.prompt {
        eprint!("{prompt}");
        let _ = std::io::stderr().flush();
    }

    let echo_guard = (options.silent && sys::isatty(0)).then(|| sys::disable_echo(0));
    let (bytes, ending) = read_input(&options);
    if let Some(Some(saved)) = echo_guard {
        sys::restore_terminal(0, &saved);
        eprintln!();
    }

    let mut line = String::from_utf8_lossy(&bytes).into_owned();
    if !options.raw {
        line = remove_backslashes(&line);
    }

    if let Some(array) = &options.array {
        let ifs = shell
            .get_variable("IFS")
            .unwrap_or_else(|| DEFAULT_IFS.into());
        let fields = split_ifs(&line, &ifs, None);
        if let Err(message) = shell.set_array(array, Array::indexed(fields)) {
            shell::error(format!("read: {message}"));
            return 1;
        }
    } else {
        assign_fields(shell, &names, &line);
    }
    match ending {
        Ending::TimedOut => 142,
        Ending::EndOfInput if bytes.is_empty() => 1,
        Ending::EndOfInput => 1,
        Ending::Delimiter | Ending::Count => 0,
    }
}

pub(super) fn mapfile(shell: &mut Shell, args: &[String]) -> i32 {
    let mut trim = false;
    let mut limit = None;
    let mut skip = 0usize;
    let mut name = "MAPFILE".to_string();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        index += 1;
        match argument.as_str() {
            "-t" => trim = true,
            "-n" | "-s" => {
                let Some(value) = args
                    .get(index)
                    .and_then(|value| value.parse::<usize>().ok())
                else {
                    shell::error(format!("mapfile: {argument}: invalid number"));
                    return 2;
                };
                index += 1;
                if argument == "-n" {
                    limit = Some(value).filter(|value| *value > 0);
                } else {
                    skip = value;
                }
            }
            other if is_name(other) => name = other.to_string(),
            other => {
                shell::error(format!("mapfile: {other}: invalid option"));
                return 2;
            }
        }
    }
    let mut lines = Vec::new();
    let mut seen = 0;
    while limit.is_none_or(|limit| lines.len() < limit) {
        let Some(mut line) = sys::read_line(0) else {
            break;
        };
        seen += 1;
        if seen <= skip {
            continue;
        }
        if trim && line.ends_with('\n') {
            line.pop();
        }
        lines.push(line);
    }
    match shell.set_array(&name, Array::indexed(lines)) {
        Ok(()) => 0,
        Err(message) => {
            shell::error(format!("mapfile: {message}"));
            1
        }
    }
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
    let ifs = shell
        .get_variable("IFS")
        .unwrap_or_else(|| DEFAULT_IFS.into());
    let fields = split_ifs(line, &ifs, Some(names.len()));
    for (index, name) in names.iter().enumerate() {
        shell.set_variable(name, fields.get(index).map_or("", String::as_str));
    }
}

const OPTION_NAMES: &[&str] = &["errexit", "nounset", "xtrace", "pipefail", "vi"];

fn option_flag<'a>(options: &'a mut ShellOptions, name: &str) -> Option<&'a mut bool> {
    match name {
        "errexit" | "e" => Some(&mut options.errexit),
        "nounset" | "u" => Some(&mut options.nounset),
        "xtrace" | "x" => Some(&mut options.xtrace),
        "pipefail" => Some(&mut options.pipefail),
        "vi" => Some(&mut options.vi),
        _ => None,
    }
}

pub(super) fn set(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let mut variables: Vec<_> = shell.variables.iter().collect();
        variables.sort();
        for (name, value) in variables {
            let _ = writeln!(out, "{name}='{value}'");
        }
        return 0;
    }

    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        index += 1;
        if argument == "--" {
            shell.positional = args[index..].to_vec();
            return 0;
        }
        let enable = match argument.chars().next() {
            Some('-') => true,
            Some('+') => false,
            _ => {
                shell.positional = args[index - 1..].to_vec();
                return 0;
            }
        };
        for flag in argument[1..].chars() {
            if flag == 'o' {
                let Some(name) = args.get(index) else {
                    for name in OPTION_NAMES {
                        let enabled =
                            option_flag(&mut shell.options, name).is_some_and(|flag| *flag);
                        let state = if enabled { "on" } else { "off" };
                        let _ = writeln!(out, "{name:<12}{state}");
                    }
                    return 0;
                };
                index += 1;
                if name == "emacs" {
                    shell.options.vi = !enable;
                    continue;
                }
                match option_flag(&mut shell.options, name) {
                    Some(option) => *option = enable,
                    None => {
                        shell::error(format!("set: {name}: invalid option name"));
                        return 2;
                    }
                }
                continue;
            }
            match option_flag(&mut shell.options, &flag.to_string()) {
                Some(option) => *option = enable,
                None => {
                    shell::error(format!("set: -{flag}: invalid option"));
                    return 2;
                }
            }
        }
    }
    0
}

const SHOPT_NAMES: &[&str] = &[
    "dotglob",
    "expand_aliases",
    "globstar",
    "login_shell",
    "nocaseglob",
    "nullglob",
];

fn shopt_flag<'a>(options: &'a mut ShellOptions, name: &str) -> Option<&'a mut bool> {
    match name {
        "dotglob" => Some(&mut options.dotglob),
        "expand_aliases" => Some(&mut options.expand_aliases),
        "login_shell" => Some(&mut options.login_shell),
        "globstar" => Some(&mut options.globstar),
        "nocaseglob" => Some(&mut options.nocaseglob),
        "nullglob" => Some(&mut options.nullglob),
        _ => None,
    }
}

pub(super) fn shopt(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let mut setting = None;
    let mut quiet = false;
    let mut names = Vec::new();
    for argument in args {
        match argument.as_str() {
            "-s" => setting = Some(true),
            "-u" => setting = Some(false),
            "-q" => quiet = true,
            "-p" => {}
            name => names.push(name.to_string()),
        }
    }
    let listed: Vec<String> = if names.is_empty() {
        SHOPT_NAMES.iter().map(|name| name.to_string()).collect()
    } else {
        names
    };
    let mut status = 0;
    for name in &listed {
        let Some(flag) = shopt_flag(&mut shell.options, name) else {
            shell::error(format!("shopt: {name}: invalid shell option name"));
            status = 1;
            continue;
        };
        match setting {
            Some(value) => *flag = value,
            None => {
                if !*flag {
                    status = 1;
                }
                if !quiet {
                    let state = if *flag { "on" } else { "off" };
                    let _ = writeln!(out, "{name:<16}{state}");
                }
            }
        }
    }
    status
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
