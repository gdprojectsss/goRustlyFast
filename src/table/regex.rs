use std::ffi::CString;

pub(crate) struct Regex {
    compiled: Option<Box<libc::regex_t>>,
}

#[cfg(target_vendor = "apple")]
const PLATFORM_FLAGS: libc::c_int = 0o400;
#[cfg(not(target_vendor = "apple"))]
const PLATFORM_FLAGS: libc::c_int = 0;

fn shorthand(letter: char) -> Option<&'static str> {
    Some(match letter {
        'd' => "[[:digit:]]",
        'D' => "[^[:digit:]]",
        'w' => "[[:alnum:]_]",
        'W' => "[^[:alnum:]_]",
        's' => "[[:space:]]",
        'S' => "[^[:space:]]",
        't' => "\t",
        'n' => "\n",
        'r' => "\r",
        _ => return None,
    })
}

fn class_in_brackets(letter: char) -> Option<&'static str> {
    Some(match letter {
        'd' => "[:digit:]",
        'w' => "[:alnum:]_",
        's' => "[:space:]",
        't' => "\t",
        'n' => "\n",
        'r' => "\r",
        _ => return None,
    })
}

fn bracket(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    out: &mut String,
) -> Result<(), String> {
    let mut negated = false;
    let mut closing = false;
    let mut dash = false;
    let mut items = String::new();
    if chars.peek() == Some(&'^') {
        chars.next();
        negated = true;
    }
    if chars.peek() == Some(&']') {
        chars.next();
        closing = true;
    }
    loop {
        let Some(character) = chars.next() else {
            return Err("unmatched [".into());
        };
        match character {
            ']' => break,
            '[' if matches!(chars.peek(), Some(':' | '.' | '=')) => {
                let kind = chars.next().unwrap_or(':');
                items.push('[');
                items.push(kind);
                loop {
                    let Some(inner) = chars.next() else {
                        return Err("unmatched [".into());
                    };
                    items.push(inner);
                    if inner == kind && chars.peek() == Some(&']') {
                        chars.next();
                        items.push(']');
                        break;
                    }
                }
            }
            '\\' => match chars.next() {
                Some(letter @ ('D' | 'W' | 'S')) => {
                    return Err(format!(
                        "\\{letter} cannot be used inside [...], use [^...] instead"
                    ));
                }
                Some(letter) if class_in_brackets(letter).is_some() => {
                    items.push_str(class_in_brackets(letter).unwrap_or_default());
                }
                Some(']') => closing = true,
                Some('-') => dash = true,
                Some(other) => items.push(other),
                None => return Err("trailing backslash".into()),
            },
            other => items.push(other),
        }
    }
    out.push('[');
    if negated {
        out.push('^');
    }
    if closing {
        out.push(']');
    }
    out.push_str(&items);
    if dash {
        out.push('-');
    }
    out.push(']');
    Ok(())
}

pub(crate) fn translate(pattern: &str) -> Result<String, String> {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '[' => bracket(&mut chars, &mut out)?,
            '\\' => match chars.next() {
                Some(letter) if shorthand(letter).is_some() => {
                    out.push_str(shorthand(letter).unwrap_or_default());
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => return Err("trailing backslash".into()),
            },
            other => out.push(other),
        }
    }
    Ok(out)
}

fn compile_error(compiled: &libc::regex_t, code: libc::c_int) -> String {
    let mut buffer = [0u8; 256];
    let length =
        unsafe { libc::regerror(code, compiled, buffer.as_mut_ptr().cast(), buffer.len()) };
    let end = buffer[..length.min(buffer.len())]
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(0);
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}

impl Regex {
    pub(crate) fn new(pattern: &str, ignore_case: bool) -> Result<Regex, String> {
        let (pattern, ignore_case) = match pattern.strip_prefix("(?i)") {
            Some(rest) => (rest, true),
            None => (pattern, ignore_case),
        };
        if pattern.is_empty() {
            return Ok(Regex { compiled: None });
        }
        let invalid = |reason: String| format!("invalid regular expression '{pattern}': {reason}");
        let translated = translate(pattern).map_err(invalid)?;
        let source =
            CString::new(translated).map_err(|_| invalid("it contains a NUL byte".into()))?;
        let mut flags = libc::REG_EXTENDED | libc::REG_NOSUB | PLATFORM_FLAGS;
        if ignore_case {
            flags |= libc::REG_ICASE;
        }
        let mut compiled: Box<libc::regex_t> = Box::new(unsafe { std::mem::zeroed() });
        let code = unsafe { libc::regcomp(&mut *compiled, source.as_ptr(), flags) };
        if code != 0 {
            return Err(invalid(compile_error(&compiled, code)));
        }
        Ok(Regex {
            compiled: Some(compiled),
        })
    }

    pub(crate) fn is_match(&self, text: &str) -> bool {
        let Some(compiled) = &self.compiled else {
            return true;
        };
        CString::new(text).is_ok_and(|subject| unsafe {
            libc::regexec(&**compiled, subject.as_ptr(), 0, std::ptr::null_mut(), 0) == 0
        })
    }
}

impl Drop for Regex {
    fn drop(&mut self) {
        if let Some(compiled) = &mut self.compiled {
            unsafe { libc::regfree(&mut **compiled) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, text: &str) -> bool {
        Regex::new(pattern, false).unwrap().is_match(text)
    }

    #[test]
    fn perl_shorthands_work_on_every_platform() {
        assert!(matches("^\\d+$", "2024"));
        assert!(!matches("^\\d+$", "d"));
        assert!(matches("\\w+\\s\\w+", "hello world"));
        assert!(matches("^\\S+$", "no-spaces"));
        assert!(!matches("^\\S+$", "has space"));
        assert!(matches("\\bcat\\b", "a cat sat"));
        assert!(!matches("\\bcat\\b", "concatenate"));
        assert!(matches("^[\\d.]+$", "1.25"));
        assert!(matches("\\.rs$", "main.rs"));
        assert!(!matches("\\.rs$", "mainrs"));
    }

    #[test]
    fn brackets_keep_literal_specials() {
        assert!(matches("^[\\]a]+$", "]a]"));
        assert!(matches("^[a\\-z]+$", "-az"));
        assert!(!matches("^[a\\-z]+$", "b"));
        assert!(matches("^[[:upper:]]$", "Q"));
    }

    #[test]
    fn case_and_empty_patterns() {
        assert!(!matches("readme", "README.md"));
        assert!(matches("(?i)readme", "README.md"));
        assert!(Regex::new("readme", true).unwrap().is_match("README"));
        assert!(matches("", "anything"));
    }

    #[test]
    fn invalid_patterns_are_errors() {
        assert!(Regex::new("(open", false).is_err());
        assert!(Regex::new("[abc", false).is_err());
        assert!(Regex::new("trailing\\", false).is_err());
        assert!(Regex::new("[\\W]", false).is_err());
        assert!(
            Regex::new("a{2,1}", false)
                .err()
                .unwrap_or_default()
                .starts_with("invalid regular expression 'a{2,1}'")
        );
    }
}
