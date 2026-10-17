use crate::bytes;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Echo,
    PrintfFormat,
    PrintfArgument,
    AnsiC,
}

pub(crate) struct Expanded {
    pub text: String,
    pub stopped: bool,
    pub errors: Vec<String>,
}

enum Step {
    Push(u32),
    Byte(u8),
    Literal(usize),
    Stop,
    Missing(char),
}

pub(crate) fn expand(input: &str, mode: Mode) -> Expanded {
    let chars: Vec<char> = input.chars().collect();
    let mut expanded = Expanded {
        text: String::with_capacity(input.len()),
        stopped: false,
        errors: Vec::new(),
    };
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '\\' || index + 1 == chars.len() {
            expanded.text.push(chars[index]);
            index += 1;
            continue;
        }
        let (step, consumed) = escape(&chars[index + 1..], mode);
        match step {
            Step::Push(value) => push_code_point(&mut expanded.text, value),
            Step::Byte(byte) => bytes::push_byte(&mut expanded.text, byte),
            Step::Literal(count) => expanded.text.extend(&chars[index..=index + count]),
            Step::Stop => {
                expanded.stopped = true;
                break;
            }
            Step::Missing(letter) => {
                let kind = if letter == 'x' { "hex" } else { "unicode" };
                expanded
                    .errors
                    .push(format!("missing {kind} digit for \\{letter}"));
                expanded.text.push('\\');
                expanded.text.push(letter);
            }
        }
        index += 1 + consumed;
    }
    expanded.text = bytes::canonical(&expanded.text);
    if mode == Mode::AnsiC
        && let Some(end) = expanded.text.find('\0')
    {
        expanded.text.truncate(end);
    }
    expanded
}

fn escape(rest: &[char], mode: Mode) -> (Step, usize) {
    let letter = rest[0];
    let simple = match letter {
        'a' => Some(0x07),
        'b' => Some(0x08),
        'e' | 'E' => Some(0x1b),
        'f' => Some(0x0c),
        'n' => Some(b'\n'),
        'r' => Some(b'\r'),
        't' => Some(b'\t'),
        'v' => Some(0x0b),
        '\\' => Some(b'\\'),
        '"' | '\'' | '?' if matches!(mode, Mode::PrintfFormat | Mode::AnsiC) => Some(letter as u8),
        _ => None,
    };
    if let Some(byte) = simple {
        return (Step::Byte(byte), 1);
    }
    match letter {
        '0' if matches!(mode, Mode::Echo | Mode::PrintfArgument) => octal(&rest[1..], 3, 1),
        '0'..='7' if mode != Mode::Echo => octal(rest, 3, 0),
        'x' => numeric(rest, 16, 2, mode, |value| Step::Byte(value as u8)),
        'u' => numeric(rest, 16, 4, mode, Step::Push),
        'U' => numeric(rest, 16, 8, mode, Step::Push),
        'c' => control(rest, mode),
        _ => (Step::Literal(1), 1),
    }
}

fn digits(rest: &[char], radix: u32, limit: usize) -> (u32, usize) {
    let mut value = 0u32;
    let mut count = 0;
    while count < limit
        && let Some(digit) = rest.get(count).and_then(|next| next.to_digit(radix))
    {
        value = value.wrapping_mul(radix).wrapping_add(digit);
        count += 1;
    }
    (value, count)
}

fn octal(rest: &[char], limit: usize, offset: usize) -> (Step, usize) {
    let (value, count) = digits(rest, 8, limit);
    (Step::Byte((value & 0xFF) as u8), offset + count)
}

fn numeric(
    rest: &[char],
    radix: u32,
    limit: usize,
    mode: Mode,
    step: fn(u32) -> Step,
) -> (Step, usize) {
    let (value, count) = digits(&rest[1..], radix, limit);
    if count > 0 {
        return (step(value), 1 + count);
    }
    match mode {
        Mode::PrintfFormat | Mode::PrintfArgument => (Step::Missing(rest[0]), 1),
        Mode::Echo | Mode::AnsiC => (Step::Literal(1), 1),
    }
}

fn control(rest: &[char], mode: Mode) -> (Step, usize) {
    match mode {
        Mode::Echo | Mode::PrintfArgument => (Step::Stop, 1),
        Mode::PrintfFormat => (Step::Literal(1), 1),
        Mode::AnsiC => match rest.get(1) {
            None => (Step::Literal(1), 1),
            Some('\\') if rest.get(2) == Some(&'\\') => (Step::Byte(0x1c), 3),
            Some('?') => (Step::Byte(0x7f), 2),
            Some(&character) => match u8::try_from(u32::from(character)) {
                Ok(byte) => (Step::Byte(byte & 0x1f), 2),
                Err(_) => (Step::Literal(2), 2),
            },
        },
    }
}

fn push_code_point(text: &mut String, value: u32) {
    match char::from_u32(value) {
        Some(character) if bytes::raw_byte(character).is_none() => text.push(character),
        _ => {
            for byte in extended_utf8(value) {
                bytes::push_byte(text, byte);
            }
        }
    }
}

fn extended_utf8(value: u32) -> Vec<u8> {
    let width = match value {
        0..=0x7F => return vec![value as u8],
        0x80..=0x7FF => 2,
        0x800..=0xFFFF => 3,
        0x10000..=0x1F_FFFF => 4,
        0x20_0000..=0x3FF_FFFF => 5,
        _ => 6,
    };
    let lead_marks: [u8; 7] = [0, 0, 0xC0, 0xE0, 0xF0, 0xF8, 0xFC];
    let mut encoded = vec![0u8; width];
    let mut remaining = value;
    for slot in encoded.iter_mut().skip(1).rev() {
        *slot = 0x80 | (remaining & 0x3F) as u8;
        remaining >>= 6;
    }
    encoded[0] = lead_marks[width] | remaining as u8;
    encoded
}

pub(crate) fn needs_ansi_quote(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_ascii_control() || bytes::raw_byte(character).is_some())
}

pub(crate) fn ansi_quote(text: &str) -> String {
    let mut quoted = String::from("$'");
    for character in text.chars() {
        match (bytes::raw_byte(character), character) {
            (Some(byte), _) => quoted.push_str(&format!("\\{byte:03o}")),
            (None, '\'' | '\\') => {
                quoted.push('\\');
                quoted.push(character);
            }
            (None, '\t') => quoted.push_str("\\t"),
            (None, '\n') => quoted.push_str("\\n"),
            (None, '\r') => quoted.push_str("\\r"),
            (None, '\x1b') => quoted.push_str("\\E"),
            (None, '\x07') => quoted.push_str("\\a"),
            (None, '\x08') => quoted.push_str("\\b"),
            (None, '\x0b') => quoted.push_str("\\v"),
            (None, '\x0c') => quoted.push_str("\\f"),
            (None, control) if control.is_ascii_control() => {
                quoted.push_str(&format!("\\{:03o}", u32::from(control)));
            }
            (None, other) => quoted.push(other),
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_of(input: &str, mode: Mode) -> Vec<u8> {
        bytes::encode(&expand(input, mode).text).into_owned()
    }

    #[test]
    fn high_bytes_stay_bytes() {
        assert_eq!(bytes_of("\\xff", Mode::PrintfFormat), b"\xff");
        assert_eq!(bytes_of("\\xc3\\xa9", Mode::PrintfFormat), "é".as_bytes());
        assert_eq!(expand("\\303\\251", Mode::PrintfFormat).text, "é");
        assert_eq!(bytes_of("\\0303\\0251", Mode::Echo), "é".as_bytes());
    }

    #[test]
    fn octal_rules_differ_by_mode() {
        assert_eq!(expand("\\101", Mode::Echo).text, "\\101");
        assert_eq!(expand("\\0101", Mode::Echo).text, "A");
        assert_eq!(expand("\\0101", Mode::PrintfFormat).text, "\x081");
        assert_eq!(expand("\\101\\0101", Mode::PrintfArgument).text, "AA");
        assert_eq!(bytes_of("\\777", Mode::AnsiC), b"\xff");
    }

    #[test]
    fn unicode_escapes() {
        assert_eq!(expand("\\u00e9\\U0001F600", Mode::AnsiC).text, "é😀");
        assert_eq!(bytes_of("\\U110000", Mode::AnsiC), b"\xf4\x90\x80\x80");
        assert_eq!(bytes_of("\\uD800", Mode::Echo), b"\xed\xa0\x80");
        assert_eq!(
            bytes_of("\\U7FFFFFFF", Mode::Echo),
            b"\xfd\xbf\xbf\xbf\xbf\xbf"
        );
        assert_eq!(expand("\\u", Mode::Echo).text, "\\u");
    }

    #[test]
    fn missing_digits_are_reported_by_printf() {
        let expanded = expand("a\\x", Mode::PrintfFormat);
        assert_eq!(expanded.text, "a\\x");
        assert_eq!(expanded.errors, ["missing hex digit for \\x"]);
        assert!(expand("\\x", Mode::AnsiC).errors.is_empty());
    }

    #[test]
    fn control_escapes() {
        let stopped = expand("a\\cb", Mode::Echo);
        assert_eq!(stopped.text, "a");
        assert!(stopped.stopped);
        assert_eq!(expand("a\\cb", Mode::PrintfFormat).text, "a\\cb");
        assert_eq!(
            expand("\\cA\\c?\\c[\\c1", Mode::AnsiC).text,
            "\x01\x7f\x1b\x11"
        );
        assert_eq!(expand("\\c\\\\x", Mode::AnsiC).text, "\x1cx");
        assert_eq!(expand("a\\c", Mode::AnsiC).text, "a\\c");
    }

    #[test]
    fn quotes_depend_on_mode() {
        assert_eq!(expand("\\\"\\'\\?", Mode::PrintfFormat).text, "\"'?");
        assert_eq!(
            expand("\\\"\\'\\?", Mode::PrintfArgument).text,
            "\\\"\\'\\?"
        );
        assert_eq!(expand("\\\"\\?", Mode::Echo).text, "\\\"\\?");
    }

    #[test]
    fn ansi_c_stops_at_nul() {
        assert_eq!(expand("a\\0b", Mode::AnsiC).text, "a");
        assert_eq!(expand("a\\0b", Mode::PrintfFormat).text, "a\0b");
        assert_eq!(expand("end\\", Mode::Echo).text, "end\\");
    }
}
