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

pub(super) fn umask(args: &[String], out: &mut dyn Write) -> i32 {
    match args {
        [] => {
            let _ = writeln!(out, "{:04o}", current_umask());
            0
        }
        [flag] if flag == "-S" => {
            let _ = writeln!(out, "{}", symbolic(current_umask()));
            0
        }
        [value] => match u32::from_str_radix(value, 8) {
            Ok(mask) if mask <= 0o777 => {
                unsafe {
                    libc::umask(mask as libc::mode_t);
                }
                0
            }
            _ => {
                shell::error(format!("umask: {value}: octal number out of range"));
                1
            }
        },
        _ => {
            shell::error("umask: usage: umask [-S] [mode]");
            2
        }
    }
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
}
