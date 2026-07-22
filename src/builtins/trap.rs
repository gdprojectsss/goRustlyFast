use std::io::Write;

use crate::shell::{self, Shell};
use crate::signals::{self, Disposition};
use crate::sys;

const PSEUDO_SIGNALS: &[&str] = &["EXIT", "ERR"];

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

pub(crate) fn default_disposition(shell: &Shell, signal: libc::c_int) -> Disposition {
    if !shell.interactive {
        return Disposition::Default;
    }
    match signal {
        libc::SIGINT => Disposition::Catch,
        libc::SIGQUIT | libc::SIGTSTP | libc::SIGTTIN | libc::SIGTTOU => Disposition::Ignore,
        _ => Disposition::Default,
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub(super) fn run(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    match args.first().map(String::as_str) {
        None | Some("-p") => {
            for (name, command) in &shell.traps {
                let _ = writeln!(out, "trap -- {} {name}", quote(command));
            }
            return 0;
        }
        Some("-l") => {
            let names: Vec<_> = sys::SIGNALS.iter().map(|(name, _)| *name).collect();
            let _ = writeln!(out, "{}", names.join(" "));
            return 0;
        }
        _ => {}
    }

    let reset_only = args.len() == 1 || args[0].parse::<u32>().is_ok();
    let (action, names) = if reset_only && args[0] != "-" {
        (None, args)
    } else {
        let action = (args[0] != "-").then(|| args[0].clone());
        (action, &args[1..])
    };

    let mut status = 0;
    for name in names {
        let Some(canonical) = canonical(name) else {
            shell::error(format!("trap: {name}: invalid signal specification"));
            status = 1;
            continue;
        };
        let signal = sys::signal_number(&canonical);
        match &action {
            Some(command) => {
                shell.traps.insert(canonical, command.clone());
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
        assert_eq!(canonical("BOGUS"), None);
    }

    #[test]
    fn quoting_for_listing() {
        assert_eq!(quote("echo 'hi'"), "'echo '\\''hi'\\'''");
    }
}
