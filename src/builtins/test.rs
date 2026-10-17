use std::ffi::CString;
use std::os::unix::fs::{FileTypeExt, MetadataExt};

use crate::shell::{self, Shell};
use crate::sys;

const MAX_GROUPS: usize = 32;

pub(crate) const UNARY: &[&str] = &[
    "-e", "-a", "-f", "-d", "-r", "-w", "-x", "-s", "-L", "-h", "-p", "-S", "-b", "-c", "-n", "-z",
    "-t", "-v", "-O", "-G", "-N", "-k", "-u", "-g",
];
pub(crate) const BINARY: &[&str] = &[
    "=", "==", "!=", "<", ">", "-eq", "-ne", "-lt", "-le", "-gt", "-ge", "-nt", "-ot", "-ef",
];

fn accessible(path: &str, mode: libc::c_int) -> bool {
    let Ok(path) = CString::new(path) else {
        return false;
    };
    unsafe { libc::access(path.as_ptr(), mode) == 0 }
}

pub(crate) fn unary(shell: &Shell, operator: &str, operand: &str) -> bool {
    let metadata = || std::fs::metadata(operand).ok();
    match operator {
        "-e" | "-a" => metadata().is_some(),
        "-O" => metadata().is_some_and(|metadata| metadata.uid() == unsafe { libc::geteuid() }),
        "-G" => metadata().is_some_and(|metadata| metadata.gid() == unsafe { libc::getegid() }),
        "-N" => metadata().is_some_and(|metadata| metadata.mtime() > metadata.atime()),
        "-k" => metadata().is_some_and(|metadata| metadata.mode() & 0o1000 != 0),
        "-u" => metadata().is_some_and(|metadata| metadata.mode() & 0o4000 != 0),
        "-g" => metadata().is_some_and(|metadata| metadata.mode() & 0o2000 != 0),
        "-f" => metadata().is_some_and(|metadata| metadata.is_file()),
        "-d" => metadata().is_some_and(|metadata| metadata.is_dir()),
        "-s" => metadata().is_some_and(|metadata| metadata.len() > 0),
        "-r" => accessible(operand, libc::R_OK),
        "-w" => accessible(operand, libc::W_OK),
        "-x" => accessible(operand, libc::X_OK),
        "-L" | "-h" => std::fs::symlink_metadata(operand)
            .is_ok_and(|metadata| metadata.file_type().is_symlink()),
        "-p" => metadata().is_some_and(|metadata| metadata.file_type().is_fifo()),
        "-S" => metadata().is_some_and(|metadata| metadata.file_type().is_socket()),
        "-b" => metadata().is_some_and(|metadata| metadata.file_type().is_block_device()),
        "-c" => metadata().is_some_and(|metadata| metadata.file_type().is_char_device()),
        "-n" => !operand.is_empty(),
        "-z" => operand.is_empty(),
        "-t" => operand.parse().is_ok_and(sys::isatty),
        "-v" => shell.variable_is_set(operand),
        _ => false,
    }
}

fn integer(text: &str) -> Result<i64, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("{text}: integer expression expected"))
}

fn modified(path: &str) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

pub(crate) fn binary(operator: &str, left: &str, right: &str) -> Result<bool, String> {
    Ok(match operator {
        "=" | "==" => left == right,
        "!=" => left != right,
        "<" => left < right,
        ">" => left > right,
        "-eq" => integer(left)? == integer(right)?,
        "-ne" => integer(left)? != integer(right)?,
        "-lt" => integer(left)? < integer(right)?,
        "-le" => integer(left)? <= integer(right)?,
        "-gt" => integer(left)? > integer(right)?,
        "-ge" => integer(left)? >= integer(right)?,
        "-nt" => match (modified(left), modified(right)) {
            (Some(left), Some(right)) => left > right,
            (Some(_), None) => true,
            _ => false,
        },
        "-ot" => match (modified(left), modified(right)) {
            (Some(left), Some(right)) => left < right,
            (None, Some(_)) => true,
            _ => false,
        },
        "-ef" => match (std::fs::metadata(left), std::fs::metadata(right)) {
            (Ok(left), Ok(right)) => left.dev() == right.dev() && left.ino() == right.ino(),
            _ => false,
        },
        other => return Err(format!("{other}: unknown operator")),
    })
}

struct Evaluator<'a> {
    shell: &'a Shell,
    args: &'a [String],
    position: usize,
}

impl Evaluator<'_> {
    fn peek(&self, offset: usize) -> Option<&str> {
        self.args.get(self.position + offset).map(String::as_str)
    }

    fn remaining(&self) -> usize {
        self.args.len() - self.position
    }

    fn take(&mut self) -> Result<&str, String> {
        let argument = self
            .args
            .get(self.position)
            .ok_or_else(|| "argument expected".to_string())?;
        self.position += 1;
        Ok(argument)
    }

    fn or(&mut self) -> Result<bool, String> {
        let mut value = self.and()?;
        while self.peek(0) == Some("-o") {
            self.position += 1;
            let right = self.and()?;
            value = value || right;
        }
        Ok(value)
    }

    fn and(&mut self) -> Result<bool, String> {
        let mut value = self.not()?;
        while self.peek(0) == Some("-a") {
            self.position += 1;
            let right = self.not()?;
            value = value && right;
        }
        Ok(value)
    }

    fn not(&mut self) -> Result<bool, String> {
        if self.peek(0) == Some("!") && self.remaining() > 1 {
            self.position += 1;
            return Ok(!self.not()?);
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<bool, String> {
        if self.remaining() >= 3
            && let Some(operator) = self.peek(1)
            && BINARY.contains(&operator)
        {
            let left = self.take()?.to_string();
            let operator = self.take()?.to_string();
            let right = self.take()?.to_string();
            return binary(&operator, &left, &right);
        }
        if self.peek(0) == Some("(") && self.remaining() > 1 {
            self.position += 1;
            let value = self.or()?;
            if self.take()? != ")" {
                return Err("')' expected".into());
            }
            return Ok(value);
        }
        if self.remaining() >= 2
            && let Some(operator) = self.peek(0)
            && UNARY.contains(&operator)
        {
            let operator = self.take()?.to_string();
            let operand = self.take()?.to_string();
            return Ok(unary(self.shell, &operator, &operand));
        }
        Ok(!self.take()?.is_empty())
    }
}

pub(crate) fn evaluate(shell: &Shell, args: &[String]) -> Result<bool, String> {
    if args.is_empty() {
        return Ok(false);
    }
    let mut evaluator = Evaluator {
        shell,
        args,
        position: 0,
    };
    let value = evaluator.or()?;
    match evaluator.peek(0) {
        None => Ok(value),
        Some(extra) => Err(format!("{extra}: unexpected argument")),
    }
}

pub(super) fn run(shell: &Shell, command: &str, args: &[String]) -> i32 {
    let args = if command == "[" {
        match args.split_last() {
            Some((last, rest)) if last == "]" => rest,
            _ => {
                shell::error("[: missing ']'");
                return 2;
            }
        }
    } else {
        args
    };
    match evaluate(shell, args) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(message) => {
            shell::error(format!("{command}: {message}"));
            2
        }
    }
}

pub(crate) fn regex_match(text: &str, pattern: &str) -> Result<Option<Vec<String>>, String> {
    let pattern = CString::new(pattern).map_err(|_| "invalid regular expression".to_string())?;
    let subject = CString::new(text).map_err(|_| "invalid text".to_string())?;
    unsafe {
        let mut compiled: libc::regex_t = std::mem::zeroed();
        if libc::regcomp(&mut compiled, pattern.as_ptr(), libc::REG_EXTENDED) != 0 {
            return Err("invalid regular expression".into());
        }
        let groups = MAX_GROUPS;
        let mut matches = vec![
            libc::regmatch_t {
                rm_so: -1,
                rm_eo: -1
            };
            groups
        ];
        let result = libc::regexec(&compiled, subject.as_ptr(), groups, matches.as_mut_ptr(), 0);
        libc::regfree(&mut compiled);
        if result != 0 {
            return Ok(None);
        }
        let used = matches
            .iter()
            .rposition(|found| found.rm_so >= 0)
            .map_or(0, |last| last + 1);
        Ok(Some(
            matches[..used]
                .iter()
                .map(|found| {
                    if found.rm_so < 0 {
                        String::new()
                    } else {
                        text.get(found.rm_so as usize..found.rm_eo as usize)
                            .unwrap_or_default()
                            .to_string()
                    }
                })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(args: &str) -> Result<bool, String> {
        let args: Vec<String> = args.split_whitespace().map(String::from).collect();
        evaluate(&Shell::new(), &args)
    }

    #[test]
    fn strings_and_numbers() {
        assert_eq!(check("abc = abc"), Ok(true));
        assert_eq!(check("abc != abc"), Ok(false));
        assert_eq!(check("-n x"), Ok(true));
        assert_eq!(check("-z x"), Ok(false));
        assert_eq!(check("3 -lt 10"), Ok(true));
        assert_eq!(check("10 -le 3"), Ok(false));
        assert!(check("a -eq 1").is_err());
    }

    #[test]
    fn files() {
        assert_eq!(check("-d src"), Ok(true));
        assert_eq!(check("-f src"), Ok(false));
        assert_eq!(check("-f Cargo.toml"), Ok(true));
        assert_eq!(check("-e /nonexistent/path"), Ok(false));
    }

    #[test]
    fn logic_and_grouping() {
        assert_eq!(check("! -d src"), Ok(false));
        assert_eq!(check("-d src -a -f Cargo.toml"), Ok(true));
        assert_eq!(check("-d nope -o 1 -eq 1"), Ok(true));
        assert_eq!(check("( 1 -eq 2 ) -o x"), Ok(true));
        assert_eq!(check(""), Ok(false));
        assert_eq!(check("word"), Ok(true));
        assert_eq!(check("!"), Ok(true));
    }

    #[test]
    fn regular_expressions() {
        let groups = regex_match("v1.42.0", r"^v([0-9]+)\.([0-9]+)")
            .unwrap()
            .unwrap();
        assert_eq!(groups, ["v1.42", "1", "42"]);
        assert_eq!(regex_match("abc", "^x").unwrap(), None);
        assert!(regex_match("abc", "(").is_err());
    }
}
