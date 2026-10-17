use std::io::Write;

use crate::shell::{self, Shell};
use crate::variables::single_quote;

const ALIAS_USAGE: &str = "alias: usage: alias [-p] [name[=value] ... ]";
const UNALIAS_USAGE: &str = "unalias: usage: unalias [-a] name [name ...]";

enum Parsed<'a> {
    Run { flag: bool, names: &'a [String] },
    Invalid(char),
}

fn parse<'a>(args: &'a [String], flag_letter: char) -> Parsed<'a> {
    let mut flag = false;
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        let Some(letters) = argument.strip_prefix('-').filter(|rest| !rest.is_empty()) else {
            break;
        };
        if let Some(other) = letters.chars().find(|letter| *letter != flag_letter) {
            return Parsed::Invalid(other);
        }
        flag = true;
        index += 1;
    }
    Parsed::Run {
        flag,
        names: &args[index..],
    }
}

fn invalid_option(name: &str, option: char, usage: &str) -> i32 {
    shell::error(format!("{name}: -{option}: invalid option"));
    eprintln!("{usage}");
    2
}

fn is_alias_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|character| " \t\n()<>;&|\"'`$/\\".contains(character))
}

fn print_alias(out: &mut dyn Write, name: &str, value: &str) {
    let _ = writeln!(out, "alias {name}={}", single_quote(value));
}

fn print_all(shell: &Shell, out: &mut dyn Write) {
    let mut sorted: Vec<_> = shell.aliases.iter().collect();
    sorted.sort_by_key(|(key, _)| *key);
    for (name, value) in sorted {
        print_alias(out, name, value);
    }
}

pub(super) fn alias(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let (print, names) = match parse(args, 'p') {
        Parsed::Run { flag, names } => (flag, names),
        Parsed::Invalid(option) => return invalid_option("alias", option, ALIAS_USAGE),
    };
    if names.is_empty() || print {
        print_all(shell, out);
    }
    let mut status = 0;
    for argument in names {
        match argument.split_once('=') {
            Some((name, value)) if !name.is_empty() => {
                if is_alias_name(name) {
                    shell.aliases.insert(name.into(), value.into());
                } else {
                    shell::error(format!("alias: `{name}': invalid alias name"));
                    status = 1;
                }
            }
            _ => match shell.aliases.get(argument.as_str()) {
                Some(value) => print_alias(out, argument, value),
                None => {
                    shell::error(format!("alias: {argument}: not found"));
                    status = 1;
                }
            },
        }
    }
    status
}

pub(super) fn unalias(shell: &mut Shell, args: &[String]) -> i32 {
    let (all, names) = match parse(args, 'a') {
        Parsed::Run { flag, names } => (flag, names),
        Parsed::Invalid(option) => return invalid_option("unalias", option, UNALIAS_USAGE),
    };
    if all {
        shell.aliases.clear();
        return 0;
    }
    if names.is_empty() {
        eprintln!("{UNALIAS_USAGE}");
        return 2;
    }
    let mut status = 0;
    for name in names {
        if shell.aliases.remove(name.as_str()).is_none() {
            shell::error(format!("unalias: {name}: not found"));
            status = 1;
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn options_end_at_double_dash_and_first_name() {
        let args = strings(&["-p", "--", "-a=b"]);
        let Parsed::Run { flag, names } = parse(&args, 'p') else {
            panic!("expected names");
        };
        assert!(flag);
        assert_eq!(names, ["-a=b"]);
    }

    #[test]
    fn unknown_option_is_reported() {
        let args = strings(&["-px"]);
        assert!(matches!(parse(&args, 'p'), Parsed::Invalid('x')));
    }

    #[test]
    fn alias_names_reject_shell_metacharacters() {
        assert!(is_alias_name("echo-x"));
        assert!(!is_alias_name("a/b"));
        assert!(!is_alias_name("a b"));
        assert!(!is_alias_name(""));
    }
}
