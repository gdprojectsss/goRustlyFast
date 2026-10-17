use crate::shell::Shell;

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn decode(shell: &Shell, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        let Some(escape) = chars.next() else {
            out.push('\\');
            break;
        };
        match escape {
            'a' => out.push('\x07'),
            'e' => out.push('\x1b'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            '[' | ']' => {}
            '$' => out.push(if unsafe { libc::geteuid() } == 0 {
                '#'
            } else {
                '$'
            }),
            'u' => out.push_str(&user_name(shell)),
            'h' => out.push_str(host_name().split('.').next().unwrap_or_default()),
            'H' => out.push_str(&host_name()),
            'w' => out.push_str(&working_directory(shell, false)),
            'W' => out.push_str(&working_directory(shell, true)),
            's' => out.push_str("grfsh"),
            'v' | 'V' => out.push_str(env!("CARGO_PKG_VERSION")),
            'j' => out.push_str(&shell.jobs.len().to_string()),
            '!' | '#' => out.push_str(&(shell.history.len() + 1).to_string()),
            't' | 'T' | '@' | 'A' | 'd' => out.push_str(&clock(escape)),
            digit @ '0'..='7' => {
                let mut value = digit.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match chars.peek().and_then(|next| next.to_digit(8)) {
                        Some(next) => {
                            value = value * 8 + next;
                            chars.next();
                        }
                        None => break,
                    }
                }
                out.push(char::from_u32(value).unwrap_or('\u{fffd}'));
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

fn user_name(shell: &Shell) -> String {
    shell
        .get_variable("USER")
        .or_else(|| shell.get_variable("LOGNAME"))
        .unwrap_or_default()
}

fn host_name() -> String {
    crate::sys::hostname().unwrap_or_default()
}

fn working_directory(shell: &Shell, base_only: bool) -> String {
    let directory = shell.get_variable("PWD").unwrap_or_default();
    let home = shell.get_variable("HOME").unwrap_or_default();
    if !home.is_empty() && directory == home {
        return "~".into();
    }
    if base_only {
        return match directory.rsplit_once('/') {
            Some((_, "")) | None => directory,
            Some((_, base)) => base.to_string(),
        };
    }
    match directory.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && rest.starts_with('/') => format!("~{rest}"),
        _ => directory,
    }
}

fn clock(format: char) -> String {
    let mut now: libc::time_t = 0;
    let mut parts: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::time(&mut now);
        libc::localtime_r(&now, &mut parts);
    }
    let twelve = match parts.tm_hour % 12 {
        0 => 12,
        hour => hour,
    };
    match format {
        't' => format!(
            "{:02}:{:02}:{:02}",
            parts.tm_hour, parts.tm_min, parts.tm_sec
        ),
        'T' => format!("{twelve:02}:{:02}:{:02}", parts.tm_min, parts.tm_sec),
        '@' => {
            let suffix = if parts.tm_hour < 12 { "AM" } else { "PM" };
            format!("{twelve:02}:{:02} {suffix}", parts.tm_min)
        }
        'A' => format!("{:02}:{:02}", parts.tm_hour, parts.tm_min),
        _ => format!(
            "{} {} {:02}",
            WEEKDAYS[parts.tm_wday.rem_euclid(7) as usize],
            MONTHS[parts.tm_mon.rem_euclid(12) as usize],
            parts.tm_mday
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_prompt_escapes() {
        let mut shell = Shell::new();
        shell.set_variable("USER", "ada");
        shell.set_variable("HOME", "/home/ada");
        shell.set_variable("PWD", "/home/ada/src");
        assert_eq!(decode(&shell, "\\u:\\w \\W\\n"), "ada:~/src src\n");
        assert_eq!(decode(&shell, "\\[\\e[1m\\]x\\101"), "\x1b[1mxA");
    }
}
