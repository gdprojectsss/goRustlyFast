use std::io::Write;

use crate::shell;

fn current_umask() -> u32 {
    unsafe {
        let mask = libc::umask(0o022);
        libc::umask(mask);
        mask as u32
    }
}

fn symbolic(mask: u32) -> String {
    let class = |shift: u32| {
        let allowed = !mask >> shift & 0o7;
        let mut letters = String::new();
        for (bit, letter) in [(0o4, 'r'), (0o2, 'w'), (0o1, 'x')] {
            if allowed & bit != 0 {
                letters.push(letter);
            }
        }
        letters
    };
    format!("u={},g={},o={}", class(6), class(3), class(0))
}

fn permission_bits(letters: &str, current: u32, mask: u32) -> Result<u32, char> {
    let mut bits = 0;
    for letter in letters.chars() {
        bits |= match letter {
            'r' => 0o444,
            'w' => 0o222,
            'x' | 'X' => 0o111,
            's' | 't' => 0,
            'u' => copy_class(current, 6),
            'g' => copy_class(current, 3),
            'o' => copy_class(current, 0),
            other => return Err(other),
        };
    }
    Ok(bits & mask)
}

fn copy_class(permissions: u32, shift: u32) -> u32 {
    (permissions >> shift & 0o7) * 0o111
}

fn apply_symbolic(mode: &str, mask: u32) -> Result<u32, String> {
    let mut permissions = !mask & 0o777;
    for clause in mode.split(',') {
        let who_end = clause
            .find(|character: char| !"ugoa".contains(character))
            .unwrap_or(clause.len());
        let who = clause[..who_end].chars().fold(0, |bits, letter| {
            bits | match letter {
                'u' => 0o700,
                'g' => 0o070,
                'o' => 0o007,
                _ => 0o777,
            }
        });
        let who = if who == 0 { 0o777 } else { who };
        let mut rest = &clause[who_end..];
        if rest.is_empty() {
            return Err("` ': invalid symbolic mode operator".into());
        }
        while let Some(operator) = rest.chars().next() {
            if !"+-=".contains(operator) {
                return Err(format!("`{operator}': invalid symbolic mode operator"));
            }
            let letters_end = rest[1..]
                .find(|character: char| "+-=".contains(character))
                .map_or(rest.len(), |offset| offset + 1);
            let bits = permission_bits(&rest[1..letters_end], permissions, who)
                .map_err(|letter| format!("`{letter}': invalid symbolic mode character"))?;
            permissions = match operator {
                '+' => permissions | bits,
                '-' => permissions & !bits,
                _ => (permissions & !who) | bits,
            };
            rest = &rest[letters_end..];
        }
    }
    Ok(!permissions & 0o777)
}

fn set_umask(mask: u32) {
    unsafe {
        libc::umask(mask as libc::mode_t);
    }
}

fn parse_mode(value: &str) -> Result<u32, String> {
    if value.starts_with(|character: char| character.is_ascii_digit()) {
        return match u32::from_str_radix(value, 8) {
            Ok(mask) if mask <= 0o777 => Ok(mask),
            _ => Err(format!("{value}: octal number out of range")),
        };
    }
    apply_symbolic(value, current_umask())
}

pub(super) fn umask(args: &[String], out: &mut dyn Write) -> i32 {
    let mut symbolic_output = false;
    let mut reusable = false;
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
                'S' => symbolic_output = true,
                'p' => reusable = true,
                other => {
                    shell::error(format!("umask: -{other}: invalid option"));
                    eprintln!("umask: usage: umask [-p] [-S] [mode]");
                    return 2;
                }
            }
        }
        index += 1;
    }
    if let Some(value) = args.get(index) {
        return match parse_mode(value) {
            Ok(mask) => {
                set_umask(mask);
                0
            }
            Err(message) => {
                shell::error(format!("umask: {message}"));
                1
            }
        };
    }
    let mask = current_umask();
    let shown = if symbolic_output {
        symbolic(mask)
    } else {
        format!("{mask:04o}")
    };
    let prefix = match (reusable, symbolic_output) {
        (true, true) => "umask -S ",
        (true, false) => "umask ",
        _ => "",
    };
    let _ = writeln!(out, "{prefix}{shown}");
    0
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
type Resource = libc::__rlimit_resource_t;
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
type Resource = libc::c_int;

struct Limit {
    flag: char,
    resource: Resource,
    unit: u64,
    description: &'static str,
}

const LIMITS: &[Limit] = &[
    Limit {
        flag: 'c',
        resource: libc::RLIMIT_CORE as Resource,
        unit: 512,
        description: "core file size (blocks)",
    },
    Limit {
        flag: 'd',
        resource: libc::RLIMIT_DATA as Resource,
        unit: 1024,
        description: "data seg size (kbytes)",
    },
    Limit {
        flag: 'f',
        resource: libc::RLIMIT_FSIZE as Resource,
        unit: 512,
        description: "file size (blocks)",
    },
    Limit {
        flag: 'n',
        resource: libc::RLIMIT_NOFILE as Resource,
        unit: 1,
        description: "open files",
    },
    Limit {
        flag: 's',
        resource: libc::RLIMIT_STACK as Resource,
        unit: 1024,
        description: "stack size (kbytes)",
    },
    Limit {
        flag: 't',
        resource: libc::RLIMIT_CPU as Resource,
        unit: 1,
        description: "cpu time (seconds)",
    },
    Limit {
        flag: 'u',
        resource: libc::RLIMIT_NPROC as Resource,
        unit: 1,
        description: "max user processes",
    },
    Limit {
        flag: 'v',
        resource: libc::RLIMIT_AS as Resource,
        unit: 1024,
        description: "virtual memory (kbytes)",
    },
];

fn read_limit(limit: &Limit) -> Option<libc::rlimit> {
    let mut value = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    (unsafe { libc::getrlimit(limit.resource, &mut value) } == 0).then_some(value)
}

fn show(value: libc::rlim_t, unit: u64) -> String {
    if value == libc::RLIM_INFINITY {
        "unlimited".into()
    } else {
        (value / unit).to_string()
    }
}

pub(super) fn ulimit(args: &[String], out: &mut dyn Write) -> i32 {
    let mut hard = false;
    let mut all = false;
    let mut flag = 'f';
    let mut value = None;
    for argument in args {
        if let Some(letters) = argument.strip_prefix('-') {
            for letter in letters.chars() {
                match letter {
                    'H' => hard = true,
                    'S' => hard = false,
                    'a' => all = true,
                    other => flag = other,
                }
            }
        } else {
            value = Some(argument.clone());
        }
    }

    if all {
        for limit in LIMITS {
            if let Some(current) = read_limit(limit) {
                let shown = if hard {
                    current.rlim_max
                } else {
                    current.rlim_cur
                };
                let label = format!("{} (-{})", limit.description, limit.flag);
                let _ = writeln!(out, "{label:<32}{}", show(shown, limit.unit));
            }
        }
        return 0;
    }
    let Some(limit) = LIMITS.iter().find(|limit| limit.flag == flag) else {
        shell::error(format!("ulimit: -{flag}: invalid option"));
        return 2;
    };
    let Some(mut current) = read_limit(limit) else {
        shell::error(format!("ulimit: -{flag}: cannot read limit"));
        return 1;
    };
    let Some(value) = value else {
        let shown = if hard {
            current.rlim_max
        } else {
            current.rlim_cur
        };
        let _ = writeln!(out, "{}", show(shown, limit.unit));
        return 0;
    };
    let new = if value == "unlimited" {
        libc::RLIM_INFINITY
    } else {
        match value.parse::<u64>() {
            Ok(number) => (number * limit.unit) as libc::rlim_t,
            Err(_) => {
                shell::error(format!("ulimit: {value}: invalid number"));
                return 1;
            }
        }
    };
    if hard {
        current.rlim_max = new;
    } else {
        current.rlim_cur = new;
    }
    if unsafe { libc::setrlimit(limit.resource, &current) } != 0 {
        let error = std::io::Error::last_os_error();
        shell::error(format!("ulimit: {}", crate::sys::error_message(&error)));
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic_umask() {
        assert_eq!(symbolic(0o022), "u=rwx,g=rx,o=rx");
        assert_eq!(symbolic(0o077), "u=rwx,g=,o=");
    }

    #[test]
    fn symbolic_modes_change_the_mask() {
        assert_eq!(apply_symbolic("u=rwx,g=rx,o=", 0o022), Ok(0o027));
        assert_eq!(apply_symbolic("g+w", 0o027), Ok(0o007));
        assert_eq!(apply_symbolic("o-r,a+x", 0o002), Ok(0o006));
        assert_eq!(apply_symbolic("u=g", 0o022), Ok(0o222));
        assert!(apply_symbolic("x=r", 0o022).is_err());
    }
}
