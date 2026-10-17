use std::time::{Duration, Instant};

use crate::shell::{self, Shell};
use crate::sys::{self, WaitResult};

const USAGE: &str =
    "timeout: usage: timeout [-s signal] [-k duration] [--preserve-status] duration command [args]";

pub(crate) fn parse_duration(text: &str) -> Option<Duration> {
    let (number, unit) = match text.char_indices().last()? {
        (index, unit @ ('s' | 'm' | 'h' | 'd')) => (&text[..index], unit),
        _ => (text, 's'),
    };
    let value: f64 = number
        .parse()
        .ok()
        .filter(|value: &f64| value.is_finite() && *value >= 0.0)?;
    let seconds = match unit {
        'm' => value * 60.0,
        'h' => value * 3600.0,
        'd' => value * 86400.0,
        _ => value,
    };
    Some(Duration::try_from_secs_f64(seconds).unwrap_or(Duration::MAX))
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

struct Settings {
    signal: libc::c_int,
    kill_after: Option<Duration>,
    preserve_status: bool,
    limit: Duration,
}

fn parse(args: &[String]) -> Result<(Settings, &[String]), String> {
    let mut settings = Settings {
        signal: libc::SIGTERM,
        kill_after: None,
        preserve_status: false,
        limit: Duration::ZERO,
    };
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        let (option, inline) = match argument.split_once('=') {
            Some((option, value)) if argument.starts_with("--") => {
                (option, Some(value.to_string()))
            }
            _ => (argument.as_str(), None),
        };
        let mut value = || {
            inline.clone().or_else(|| {
                index += 1;
                args.get(index).cloned()
            })
        };
        match option {
            "-s" | "--signal" => {
                let name = value().ok_or(USAGE)?;
                settings.signal = sys::signal_number(&name)
                    .ok_or_else(|| format!("timeout: {name}: invalid signal"))?;
            }
            "-k" | "--kill-after" => {
                let text = value().ok_or(USAGE)?;
                settings.kill_after = Some(
                    parse_duration(&text)
                        .ok_or_else(|| format!("timeout: invalid duration '{text}'"))?,
                );
            }
            "--preserve-status" => settings.preserve_status = true,
            "--foreground" => {}
            "--" => {
                index += 1;
                break;
            }
            _ => break,
        }
        index += 1;
    }
    let text = args.get(index).ok_or(USAGE)?;
    settings.limit =
        parse_duration(text).ok_or_else(|| format!("timeout: invalid duration '{text}'"))?;
    let command = &args[index + 1..];
    if command.is_empty() {
        return Err(USAGE.into());
    }
    Ok((settings, command))
}

pub(super) fn run(shell: &mut Shell, args: &[String]) -> i32 {
    let (settings, command) = match parse(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            shell::error(message);
            return 125;
        }
    };
    let source: Vec<String> = command.iter().map(|word| quote(word)).collect();
    let source = source.join(" ");
    let pid = match sys::fork() {
        Ok(0) => {
            let interactive = shell.interactive;
            shell.enter_child(None, false);
            if !shell.agent {
                sys::setpgid(0, 0);
            }
            if interactive {
                sys::tcsetpgrp(sys::getpid());
            }
            let name = command[0].as_str();
            let external = !shell.functions.contains_key(name)
                && !super::is_builtin(name)
                && !crate::table::is_command(name);
            if external {
                match shell.resolve(name) {
                    Some(path) => shell.exec_external(&path, command, &[], &[]),
                    None => {
                        shell.command_not_found(name);
                        sys::exit_now(127)
                    }
                }
            }
            let status = shell.run_source(&source);
            sys::exit_now(status)
        }
        Ok(pid) => pid,
        Err(error) => {
            shell::error(format!("fork: {}", sys::error_message(&error)));
            return 125;
        }
    };
    if !shell.agent {
        sys::setpgid(pid, pid);
    }
    if shell.interactive {
        sys::tcsetpgrp(pid);
    }
    let started = Instant::now();
    let mut signaled_at = None;
    let mut killed = false;
    let result = loop {
        match sys::waitpid(pid, libc::WNOHANG) {
            Ok(Some(WaitResult::Stopped | WaitResult::Continued)) | Ok(None) => {}
            Ok(Some(result)) => break Some(result),
            Err(_) => break None,
        }
        let now = Instant::now();
        match signaled_at {
            None if now.duration_since(started) >= settings.limit => {
                let _ = sys::kill(-pid, settings.signal);
                let _ = sys::kill(-pid, libc::SIGCONT);
                signaled_at = Some(now);
            }
            Some(at)
                if !killed
                    && settings
                        .kill_after
                        .is_some_and(|after| now.duration_since(at) >= after) =>
            {
                let _ = sys::kill(-pid, libc::SIGKILL);
                killed = true;
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if signaled_at.is_some() {
        let _ = sys::kill(-pid, libc::SIGKILL);
    }
    if shell.interactive {
        sys::tcsetpgrp(shell.shell_pgid);
    }
    let status = result.map_or(125, WaitResult::status);
    match signaled_at {
        Some(_) if killed || settings.signal == libc::SIGKILL => 137,
        Some(_) if !settings.preserve_status => 124,
        _ => status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_take_units() {
        assert_eq!(parse_duration("1.5"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_duration("2m"), Some(Duration::from_secs(120)));
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration("x"), None);
        assert_eq!(parse_duration("-1"), None);
        assert_eq!(parse_duration("1e20"), Some(Duration::MAX));
    }
}
