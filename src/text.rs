fn is_wide(character: char) -> bool {
    matches!(character as u32,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD)
}

fn is_zero_width(character: char) -> bool {
    matches!(character as u32,
        0x0300..=0x036F | 0x200B..=0x200F | 0xFE00..=0xFE0F | 0x20D0..=0x20FF)
}

pub fn char_width(character: char) -> usize {
    if character.is_control() || is_zero_width(character) {
        0
    } else if is_wide(character) {
        2
    } else {
        1
    }
}

pub fn display_width(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for code in chars.by_ref() {
                    if ('@'..='~').contains(&code) {
                        break;
                    }
                }
            }
            continue;
        }
        width += char_width(character);
    }
    width
}

pub fn colors_enabled() -> bool {
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
    let dumb = crate::bytes::env_var("TERM").is_some_and(|term| term == "dumb");
    !no_color && !dumb
}

pub fn truecolor() -> bool {
    crate::bytes::env_var("COLORTERM").is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value == "truecolor" || value == "24bit"
    })
}

pub fn unicode_terminal() -> bool {
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| crate::bytes::env_var(name).filter(|value| !value.is_empty()));
    match locale {
        Some(value) => {
            let value = value.to_ascii_lowercase();
            value.contains("utf-8") || value.contains("utf8")
        }
        None => true,
    }
}

pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) {
                    break;
                }
            }
            continue;
        }
        out.push(character);
    }
    out
}

pub fn without_colors(text: String) -> String {
    if colors_enabled() {
        text
    } else {
        strip_ansi(&text)
    }
}

pub fn truncate_to_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let next = char_width(character);
        if used + next > width {
            break;
        }
        used += next;
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("\u{65e5}\u{672c}"), 4);
        assert_eq!(display_width("\x1b[1;32mok\x1b[0m"), 2);
        assert_eq!(display_width("e\u{301}"), 1);
    }

    #[test]
    fn stripping_escape_codes() {
        assert_eq!(strip_ansi("\x1b[1;31mred\x1b[0m plain"), "red plain");
        assert_eq!(strip_ansi("no codes"), "no codes");
    }

    #[test]
    fn truncation_respects_wide_characters() {
        assert_eq!(
            truncate_to_width("\u{65e5}\u{672c}\u{8a9e}", 5),
            "\u{65e5}\u{672c}"
        );
        assert_eq!(truncate_to_width("hello", 3), "hel");
    }
}
