use std::io::Write;

use crate::shell::{self, Shell, ShellOptions};

const SET_USAGE: &str =
    "set: usage: set [-abefhkmnptuvxBCEHPT] [-o option-name] [--] [-] [arg ...]";
const SHOPT_USAGE: &str = "shopt: usage: shopt [-pqsu] [-o] [optname ...]";

const SET_OPTIONS: &[&str] = &[
    "allexport",
    "braceexpand",
    "emacs",
    "errexit",
    "errtrace",
    "functrace",
    "hashall",
    "histexpand",
    "history",
    "ignoreeof",
    "interactive-comments",
    "keyword",
    "monitor",
    "noclobber",
    "noexec",
    "noglob",
    "nolog",
    "notify",
    "nounset",
    "onecmd",
    "physical",
    "pipefail",
    "posix",
    "privileged",
    "verbose",
    "vi",
    "xtrace",
];

const SHOPT_OPTIONS: &[&str] = &[
    "array_expand_once",
    "assoc_expand_once",
    "autocd",
    "bash_source_fullpath",
    "cdable_vars",
    "cdspell",
    "checkhash",
    "checkjobs",
    "checkwinsize",
    "cmdhist",
    "compat31",
    "compat32",
    "compat40",
    "compat41",
    "compat42",
    "compat43",
    "compat44",
    "complete_fullquote",
    "direxpand",
    "dirspell",
    "dotglob",
    "execfail",
    "expand_aliases",
    "extdebug",
    "extglob",
    "extquote",
    "failglob",
    "force_fignore",
    "globasciiranges",
    "globskipdots",
    "globstar",
    "gnu_errfmt",
    "histappend",
    "histreedit",
    "histverify",
    "hostcomplete",
    "huponexit",
    "inherit_errexit",
    "interactive_comments",
    "lastpipe",
    "lithist",
    "localvar_inherit",
    "localvar_unset",
    "login_shell",
    "mailwarn",
    "no_empty_cmd_completion",
    "nocaseglob",
    "nocasematch",
    "noexpand_translation",
    "nullglob",
    "patsub_replacement",
    "progcomp",
    "progcomp_alias",
    "promptvars",
    "restricted_shell",
    "shift_verbose",
    "sourcepath",
    "varredir_close",
    "xpg_echo",
];

const STORED_ON_BY_DEFAULT: &[&str] = &[
    "checkwinsize",
    "cmdhist",
    "complete_fullquote",
    "extquote",
    "force_fignore",
    "globasciiranges",
    "globskipdots",
    "hostcomplete",
    "patsub_replacement",
    "progcomp",
    "promptvars",
];

pub(crate) fn default_stored_shopts() -> u64 {
    STORED_ON_BY_DEFAULT
        .iter()
        .filter_map(|name| stored_bit(name))
        .fold(0, |bits, bit| bits | bit)
}

fn stored_bit(name: &str) -> Option<u64> {
    SHOPT_OPTIONS
        .iter()
        .position(|option| *option == name)
        .map(|position| 1u64 << position)
}

fn set_flag<'a>(options: &'a mut ShellOptions, name: &str) -> Option<&'a mut bool> {
    Some(match name {
        "allexport" => &mut options.allexport,
        "braceexpand" => &mut options.braceexpand,
        "emacs" => &mut options.emacs,
        "errexit" => &mut options.errexit,
        "errtrace" => &mut options.errtrace,
        "functrace" => &mut options.functrace,
        "hashall" => &mut options.hashall,
        "histexpand" => &mut options.histexpand,
        "history" => &mut options.history,
        "ignoreeof" => &mut options.ignoreeof,
        "interactive-comments" => &mut options.interactive_comments,
        "keyword" => &mut options.keyword,
        "monitor" => &mut options.monitor,
        "noclobber" => &mut options.noclobber,
        "noexec" => &mut options.noexec,
        "noglob" => &mut options.noglob,
        "nolog" => &mut options.nolog,
        "notify" => &mut options.notify,
        "nounset" => &mut options.nounset,
        "onecmd" => &mut options.onecmd,
        "physical" => &mut options.physical,
        "pipefail" => &mut options.pipefail,
        "posix" => &mut options.posix,
        "privileged" => &mut options.privileged,
        "verbose" => &mut options.verbose,
        "vi" => &mut options.vi,
        "xtrace" => &mut options.xtrace,
        _ => return None,
    })
}

fn letter_option(letter: char) -> Option<&'static str> {
    Some(match letter {
        'a' => "allexport",
        'b' => "notify",
        'e' => "errexit",
        'f' => "noglob",
        'h' => "hashall",
        'k' => "keyword",
        'm' => "monitor",
        'n' => "noexec",
        'p' => "privileged",
        't' => "onecmd",
        'u' => "nounset",
        'v' => "verbose",
        'x' => "xtrace",
        'B' => "braceexpand",
        'C' => "noclobber",
        'E' => "errtrace",
        'H' => "histexpand",
        'P' => "physical",
        'T' => "functrace",
        _ => return None,
    })
}

pub(super) fn set_option_state(shell: &Shell, name: &str) -> bool {
    let mut options = shell.options;
    match name {
        "emacs" => options.emacs || (shell.interactive && !options.vi),
        _ => set_flag(&mut options, name).is_some_and(|flag| *flag),
    }
}

fn change_set_option(shell: &mut Shell, name: &str, enable: bool) -> bool {
    let Some(flag) = set_flag(&mut shell.options, name) else {
        return false;
    };
    *flag = enable;
    match name {
        "vi" if enable => shell.options.emacs = false,
        "emacs" if enable => shell.options.vi = false,
        _ => {}
    }
    true
}

fn list_set_options(shell: &Shell, reusable: bool, out: &mut dyn Write) {
    for name in SET_OPTIONS {
        let enabled = set_option_state(shell, name);
        let _ = if reusable {
            let sign = if enabled { '-' } else { '+' };
            writeln!(out, "set {sign}o {name}")
        } else {
            let state = if enabled { "on" } else { "off" };
            writeln!(out, "{name:<15}\t{state}")
        };
    }
}

fn print_variables(shell: &Shell, out: &mut dyn Write) {
    let mut variables: Vec<_> = shell.variables.iter().collect();
    variables.sort();
    for (name, value) in variables {
        let _ = writeln!(out, "{name}='{value}'");
    }
}

pub(super) fn set(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    if args.is_empty() {
        print_variables(shell, out);
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
        if argument == "-" {
            shell.options.xtrace = false;
            shell.options.verbose = false;
            shell.positional = args[index..].to_vec();
            return 0;
        }
        let enable = match argument.chars().next() {
            Some('-') => true,
            Some('+') if argument.len() > 1 => false,
            _ => {
                shell.positional = args[index - 1..].to_vec();
                return 0;
            }
        };
        for letter in argument[1..].chars() {
            if letter == 'o' {
                let Some(name) = args.get(index) else {
                    list_set_options(shell, !enable, out);
                    continue;
                };
                index += 1;
                if !change_set_option(shell, name, enable) {
                    shell::error(format!("set: {name}: invalid option name"));
                    return 2;
                }
                continue;
            }
            match letter_option(letter) {
                Some(name) => {
                    change_set_option(shell, name, enable);
                }
                None => {
                    shell::error(format!("set: -{letter}: invalid option"));
                    eprintln!("{SET_USAGE}");
                    return 2;
                }
            }
        }
    }
    0
}

fn shopt_flag<'a>(options: &'a mut ShellOptions, name: &str) -> Option<&'a mut bool> {
    Some(match name {
        "dotglob" => &mut options.dotglob,
        "globstar" => &mut options.globstar,
        "nocaseglob" => &mut options.nocaseglob,
        "nullglob" => &mut options.nullglob,
        "nocasematch" => &mut options.nocasematch,
        "failglob" => &mut options.failglob,
        "lastpipe" => &mut options.lastpipe,
        "expand_aliases" => &mut options.expand_aliases,
        "extglob" => &mut options.extglob,
        "inherit_errexit" => &mut options.inherit_errexit,
        "interactive_comments" => &mut options.interactive_comments,
        "sourcepath" => &mut options.sourcepath,
        _ => return None,
    })
}

pub(crate) fn shopt_enabled(shell: &Shell, name: &str) -> bool {
    shopt_state(shell, name).unwrap_or(false)
}

fn shopt_state(shell: &Shell, name: &str) -> Option<bool> {
    let mut options = shell.options;
    if let Some(flag) = shopt_flag(&mut options, name) {
        return Some(*flag);
    }
    match name {
        "login_shell" => Some(shell.options.login_shell || shell.arg0.starts_with('-')),
        _ => stored_bit(name).map(|bit| options.stored_shopts & bit != 0),
    }
}

fn change_shopt(shell: &mut Shell, name: &str, enable: bool) -> bool {
    if let Some(flag) = shopt_flag(&mut shell.options, name) {
        *flag = enable;
        if name == "extglob" {
            crate::expand::glob::set_extglob(enable);
        }
        return true;
    }
    let Some(bit) = stored_bit(name) else {
        return false;
    };
    if enable {
        shell.options.stored_shopts |= bit;
    } else {
        shell.options.stored_shopts &= !bit;
    }
    true
}

#[derive(Default)]
struct ShoptFlags {
    set: bool,
    unset: bool,
    quiet: bool,
    reusable: bool,
    set_options: bool,
}

fn parse_shopt_flags(args: &[String]) -> Result<(ShoptFlags, &[String]), String> {
    let mut flags = ShoptFlags::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        if !argument.starts_with('-') || argument.len() < 2 {
            break;
        }
        for letter in argument[1..].chars() {
            match letter {
                's' => flags.set = true,
                'u' => flags.unset = true,
                'q' => flags.quiet = true,
                'p' => flags.reusable = true,
                'o' => flags.set_options = true,
                other => return Err(format!("-{other}")),
            }
        }
        index += 1;
    }
    Ok((flags, &args[index..]))
}

fn print_shopt_line(name: &str, enabled: bool, flags: &ShoptFlags, out: &mut dyn Write) {
    let _ = match (flags.reusable, flags.set_options) {
        (true, true) => writeln!(out, "set {}o {name}", if enabled { '-' } else { '+' }),
        (true, false) => writeln!(out, "shopt {} {name}", if enabled { "-s" } else { "-u" }),
        (false, true) => writeln!(out, "{name:<15}\t{}", if enabled { "on" } else { "off" }),
        (false, false) => writeln!(out, "{name:<20}\t{}", if enabled { "on" } else { "off" }),
    };
}

pub(super) fn shopt(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let (flags, names) = match parse_shopt_flags(args) {
        Ok(parsed) => parsed,
        Err(flag) => {
            shell::error(format!("shopt: {flag}: invalid option"));
            eprintln!("{SHOPT_USAGE}");
            return 2;
        }
    };
    if flags.set && flags.unset {
        shell::error("shopt: cannot set and unset shell options simultaneously");
        return 1;
    }
    let state = |shell: &Shell, name: &str| {
        if flags.set_options {
            SET_OPTIONS
                .contains(&name)
                .then(|| set_option_state(shell, name))
        } else {
            shopt_state(shell, name)
        }
    };
    if names.is_empty() {
        let all = if flags.set_options {
            SET_OPTIONS
        } else {
            SHOPT_OPTIONS
        };
        for name in all {
            let enabled = state(shell, name).unwrap_or(false);
            let shown = match (flags.set, flags.unset) {
                (true, _) => enabled,
                (_, true) => !enabled,
                _ => true,
            };
            if shown && !flags.quiet {
                print_shopt_line(name, enabled, &flags, out);
            }
        }
        return 0;
    }
    let mut status = 0;
    for name in names {
        let Some(enabled) = state(shell, name) else {
            let kind = if flags.set_options { "" } else { "shell " };
            shell::error(format!("shopt: {name}: invalid {kind}option name"));
            status = 1;
            continue;
        };
        if flags.set || flags.unset {
            if flags.set_options {
                change_set_option(shell, name, flags.set);
            } else {
                change_shopt(shell, name, flags.set);
            }
            continue;
        }
        if !enabled {
            status = 1;
        }
        if !flags.quiet {
            print_shopt_line(name, enabled, &flags, out);
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_set(shell: &mut Shell, args: &[&str]) -> i32 {
        let args: Vec<String> = args.iter().map(|argument| argument.to_string()).collect();
        set(shell, &args, &mut Vec::new())
    }

    #[test]
    fn letters_map_to_long_options() {
        let mut shell = Shell::new();
        assert_eq!(run_set(&mut shell, &["-fC", "-o", "posix"]), 0);
        assert!(shell.options.noglob && shell.options.noclobber && shell.options.posix);
        assert_eq!(shell.get_variable("-").as_deref(), Some("fhBC"));
        assert_eq!(run_set(&mut shell, &["-q"]), 2);
    }

    #[test]
    fn shopt_stores_options_without_their_own_field() {
        let mut shell = Shell::new();
        assert_eq!(shopt_state(&shell, "checkwinsize"), Some(true));
        assert!(change_shopt(&mut shell, "checkwinsize", false));
        assert_eq!(shopt_state(&shell, "checkwinsize"), Some(false));
        assert_eq!(shopt_state(&shell, "nosuch"), None);
        assert!(SHOPT_OPTIONS.len() <= 64);
    }
}
