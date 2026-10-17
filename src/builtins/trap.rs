use std::io::Write;

use crate::shell::{self, Shell};
use crate::signals::{self, Disposition};
use crate::sys;

const PSEUDO_SIGNALS: &[&str] = &["EXIT", "DEBUG", "ERR", "RETURN"];

pub(crate) const TERMINATING: &[libc::c_int] = &[
    libc::SIGHUP,
    libc::SIGINT,
    libc::SIGPIPE,
    libc::SIGALRM,
    libc::SIGTERM,
    libc::SIGUSR1,
    libc::SIGUSR2,
];

pub(crate) fn canonical(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    let upper = upper.strip_prefix("SIG").unwrap_or(&upper).to_string();
    if upper == "0" || upper == "EXIT" {
        return Some("EXIT".into());
    }
    if PSEUDO_SIGNALS.contains(&upper.as_str()) {
        return Some(upper);
    }
    let number = sys::signal_number(&upper)?;
    sys::signal_name(number).map(String::from)
}

pub(crate) fn is_terminating(signal: libc::c_int) -> bool {
    TERMINATING.contains(&signal)
}

pub(crate) fn default_disposition(shell: &Shell, signal: libc::c_int) -> Disposition {
    if signal == libc::SIGQUIT {
        return Disposition::Ignore;
    }
    if shell.interactive {
        return match signal {
            libc::SIGINT | libc::SIGHUP => Disposition::Catch,
            libc::SIGTSTP | libc::SIGTTIN | libc::SIGTTOU | libc::SIGPIPE => Disposition::Ignore,
            _ => Disposition::Default,
        };
    }
    if shell.agent && signal == libc::SIGPIPE {
        return Disposition::Ignore;
    }
    let exit_trap = shell
        .traps
        .get("EXIT")
        .is_some_and(|command| !command.is_empty());
    if exit_trap && is_terminating(signal) {
        Disposition::Catch
    } else {
        Disposition::Default
    }
}

pub(crate) fn refresh_terminating(shell: &Shell) {
    for &signal in TERMINATING {
        let trapped = sys::signal_name(signal).is_some_and(|name| shell.traps.contains_key(name));
        if !trapped {
            signals::set(signal, default_disposition(shell, signal));
        }
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn listing_order(name: &str) -> i32 {
    match name {
        "EXIT" => 0,
        "DEBUG" => 1000,
        "ERR" => 1001,
        "RETURN" => 1002,
        _ => sys::signal_number(name).unwrap_or(999),
    }
}

fn listing_name(name: &str) -> String {
    if PSEUDO_SIGNALS.contains(&name) {
        name.to_string()
    } else {
        format!("SIG{name}")
    }
}

fn print_traps(shell: &Shell, names: &[String], out: &mut dyn Write) -> i32 {
    let mut status = 0;
    let mut selected: Vec<String> = Vec::new();
    let traps = shell.subshell_traps.as_ref().unwrap_or(&shell.traps);
    if names.is_empty() {
        selected = traps.keys().cloned().collect();
    } else {
        for name in names {
            match canonical(name) {
                Some(canonical) => selected.push(canonical),
                None => {
                    shell::error(format!("trap: {name}: invalid signal specification"));
                    status = 1;
                }
            }
        }
    }
    selected.sort_by_key(|name| listing_order(name));
    for name in selected {
        if let Some(command) = traps.get(&name) {
            let _ = writeln!(out, "trap -- {} {}", quote(command), listing_name(&name));
        }
    }
    status
}

pub(super) fn run(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let mut args = args;
    match args.first().map(String::as_str) {
        None => return print_traps(shell, &[], out),
        Some("-p") | Some("-P") => return print_traps(shell, &args[1..], out),
        Some("-l") => {
            let _ = write!(out, "{}", sys::signal_listing());
            return 0;
        }
        Some("--") => args = &args[1..],
        Some(option) if option.len() > 1 && option.starts_with('-') => {
            shell::error(format!("trap: {option}: invalid option"));
            eprintln!("trap: usage: trap [-Plp] [[action] signal_spec ...]");
            return 2;
        }
        _ => {}
    }
    if args.is_empty() {
        return print_traps(shell, &[], out);
    }

    let reset_only = args.len() == 1 || args[0].parse::<u32>().is_ok();
    let (action, names) = if reset_only && args[0] != "-" {
        (None, args)
    } else {
        let action = (args[0] != "-").then(|| args[0].clone());
        (action, &args[1..])
    };

    shell.subshell_traps = None;
    let mut status = 0;
    for name in names {
        let Some(canonical) = canonical(name) else {
            shell::error(format!("trap: {name}: invalid signal specification"));
            status = 1;
            continue;
        };
        let signal = sys::signal_number(&canonical).filter(|_| canonical != "EXIT");
        match &action {
            Some(command) => {
                shell.traps.insert(canonical.clone(), command.clone());
                if let Some(signal) = signal {
                    let disposition = if command.is_empty() {
                        Disposition::Ignore
                    } else {
                        Disposition::Catch
                    };
                    signals::set(signal, disposition);
                }
            }
            None => {
                shell.traps.remove(&canonical);
                if let Some(signal) = signal {
                    signals::set(signal, default_disposition(shell, signal));
                }
            }
        }
        if canonical == "EXIT" {
            refresh_terminating(shell);
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_normalized() {
        assert_eq!(canonical("sigint").as_deref(), Some("INT"));
        assert_eq!(canonical("2").as_deref(), Some("INT"));
        assert_eq!(canonical("0").as_deref(), Some("EXIT"));
        assert_eq!(canonical("err").as_deref(), Some("ERR"));
        assert_eq!(canonical("debug").as_deref(), Some("DEBUG"));
        assert_eq!(canonical("RETURN").as_deref(), Some("RETURN"));
        assert_eq!(canonical("BOGUS"), None);
    }

    #[test]
    fn quoting_for_listing() {
        assert_eq!(quote("echo 'hi'"), "'echo '\\''hi'\\'''");
    }

    #[test]
    fn listing_follows_signal_numbers() {
        let mut names = vec!["RETURN", "USR1", "EXIT", "INT", "DEBUG"];
        names.sort_by_key(|name| listing_order(name));
        assert_eq!(names, ["EXIT", "INT", "USR1", "DEBUG", "RETURN"]);
        assert_eq!(listing_name("INT"), "SIGINT");
        assert_eq!(listing_name("EXIT"), "EXIT");
    }
}
