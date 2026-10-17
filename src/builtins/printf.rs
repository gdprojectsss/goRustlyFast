use std::io::Write;

use crate::bytes;
use crate::escapes;
use crate::shell::{self, Shell};
use crate::syntax::parser::is_name;
use crate::variables::Key;

const USAGE: &str = "printf: usage: printf [-v var] format [arguments]";
const LENGTH_MODIFIERS: &str = "hjlLtz";

unsafe extern "C" {
    fn tzset();
}

struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    zero: bool,
    alternate: bool,
    width: Option<usize>,
    precision: Option<usize>,
    time_format: Option<String>,
    conversion: char,
}

struct Arguments<'a> {
    values: &'a [String],
    position: usize,
    failed: bool,
    shell_started: i64,
}

enum Stop {
    Escape(String),
    Error,
}

fn integer_prefix(text: &str) -> (i64, Option<&'static str>) {
    let trimmed = text.trim_start();
    let (negative, unsigned) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };
    let (radix, digits, kind) = if let Some(hex) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        (16, hex, "invalid hex number")
    } else if unsigned.len() > 1 && unsigned.starts_with('0') {
        (8, &unsigned[1..], "invalid octal number")
    } else {
        (10, unsigned, "invalid number")
    };
    let used = digits
        .find(|character: char| !character.is_digit(radix))
        .unwrap_or(digits.len());
    let mut value: i64 = 0;
    let mut overflow = false;
    for character in digits[..used].chars() {
        let digit = i64::from(character.to_digit(radix).unwrap_or(0));
        let next = value
            .checked_mul(i64::from(radix))
            .and_then(|value| value.checked_add(digit));
        match next {
            Some(next) => value = next,
            None => {
                overflow = true;
                break;
            }
        }
    }
    if overflow {
        let limit = if negative { i64::MIN } else { i64::MAX };
        return (limit, Some("Result too large"));
    }
    let value = if negative { -value } else { value };
    let complete = used == digits.len() && (used > 0 || radix == 8);
    if trimmed.is_empty() || !complete {
        return (value, Some(kind));
    }
    (value, None)
}

fn float_prefix(text: &str) -> (f64, bool) {
    let trimmed = text.trim_start();
    if let Ok(value) = trimmed.parse::<f64>() {
        return (value, true);
    }
    let mut end = trimmed.len();
    while end > 0 {
        if let Some(value) = trimmed
            .get(..end)
            .and_then(|prefix| prefix.parse::<f64>().ok())
        {
            return (value, false);
        }
        end -= 1;
    }
    (0.0, false)
}

impl<'a> Arguments<'a> {
    fn next(&mut self) -> Option<&'a str> {
        let value = self.values.get(self.position).map(String::as_str);
        if value.is_some() {
            self.position += 1;
        }
        value
    }

    fn report(&mut self, text: &str, problem: &str) {
        shell::error(format!("printf: {text}: {problem}"));
        self.failed = true;
    }

    fn integer_text(&mut self, text: &str) -> i64 {
        if let Some(quoted) = text.strip_prefix(['\'', '"']) {
            return quoted.chars().next().map_or(0, |character| {
                bytes::raw_byte(character).map_or(character as i64, i64::from)
            });
        }
        let (value, problem) = integer_prefix(text);
        if let Some(problem) = problem {
            self.report(text, problem);
        }
        value
    }

    fn integer(&mut self) -> i64 {
        match self.next() {
            Some(text) => self.integer_text(text),
            None => 0,
        }
    }

    fn float(&mut self) -> f64 {
        let Some(text) = self.next() else {
            return 0.0;
        };
        if let Some(quoted) = text.strip_prefix(['\'', '"']) {
            return quoted
                .chars()
                .next()
                .map_or(0.0, |character| character as u32 as f64);
        }
        let (value, complete) = float_prefix(text);
        if !complete {
            self.report(text, "invalid number");
        }
        value
    }

    fn star(&mut self) -> Option<i64> {
        let text = self.next()?;
        let value = self.integer_text(text);
        if value > i64::from(i32::MAX) || value < -i64::from(i32::MAX) {
            self.report(text, "Result too large");
            return None;
        }
        Some(value)
    }
}

fn escape(text: &str, out: &mut String, in_format: bool) -> bool {
    let mode = if in_format {
        escapes::Mode::PrintfFormat
    } else {
        escapes::Mode::PrintfArgument
    };
    let expanded = escapes::expand(text, mode);
    for message in &expanded.errors {
        shell::error(format!("printf: {message}"));
    }
    out.push_str(&expanded.text);
    !expanded.stopped
}

fn shell_quote(text: &str) -> String {
    if text.is_empty() {
        return "''".into();
    }
    if escapes::needs_ansi_quote(text) {
        return escapes::ansi_quote(text);
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut previous = None;
    for character in text.chars() {
        let escaped = match character {
            ' ' | '\'' | '"' | '\\' | '|' | '&' | ';' | '(' | ')' | '<' | '>' | '!' | '{' | '}'
            | '*' | '[' | '?' | ']' | '^' | '$' | '`' | ',' => true,
            '~' => matches!(previous, None | Some('=') | Some(':')),
            '#' => previous.is_none(),
            _ => false,
        };
        if escaped {
            out.push('\\');
        }
        out.push(character);
        previous = Some(character);
    }
    out
}

fn pad(spec: &Spec, body: String, numeric: bool) -> String {
    let width = spec.width.unwrap_or(0);
    let length = body.chars().count();
    if length >= width {
        return body;
    }
    let fill = width - length;
    if spec.left {
        return body + &" ".repeat(fill);
    }
    if spec.zero && numeric {
        let sign_length = usize::from(body.starts_with(['-', '+', ' ']));
        let (sign, digits) = body.split_at(sign_length);
        return format!("{sign}{}{digits}", "0".repeat(fill));
    }
    " ".repeat(fill) + &body
}

fn signed(spec: &Spec, value: String, negative: bool) -> String {
    if negative {
        value
    } else if spec.plus {
        format!("+{value}")
    } else if spec.space {
        format!(" {value}")
    } else {
        value
    }
}

fn format_time(format: &str, seconds: i64) -> String {
    let format = if format.is_empty() { "%X" } else { format };
    let Ok(format) = std::ffi::CString::new(format) else {
        return String::new();
    };
    let time = seconds as libc::time_t;
    let mut buffer = vec![0u8; 1024];
    unsafe {
        tzset();
        let mut parts: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&time, &mut parts).is_null() {
            return String::new();
        }
        let written = libc::strftime(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            format.as_ptr(),
            &parts,
        );
        buffer.truncate(written);
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

fn time_argument(arguments: &mut Arguments) -> i64 {
    let started = arguments.shell_started;
    let value = match arguments.next() {
        Some(text) => arguments.integer_text(text),
        None => -1,
    };
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() as i64)
    };
    match value {
        -1 => now(),
        -2 => started,
        other => other,
    }
}

fn hex_float(value: f64, upper: bool) -> String {
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let magnitude = value.abs();
    let body = if magnitude == 0.0 {
        "0x0p+0".to_string()
    } else if !magnitude.is_finite() {
        return general(value, 6, upper);
    } else {
        let bits = magnitude.to_bits();
        let raw_exponent = ((bits >> 52) & 0x7ff) as i64;
        let mantissa = bits & ((1u64 << 52) - 1);
        let (lead, exponent) = if raw_exponent == 0 {
            (0, -1022)
        } else {
            (1, raw_exponent - 1023)
        };
        let fraction = format!("{mantissa:013x}");
        let fraction = fraction.trim_end_matches('0');
        let dot = if fraction.is_empty() { "" } else { "." };
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("0x{lead}{dot}{fraction}p{sign}{}", exponent.abs())
    };
    let text = format!("{sign}{body}");
    if upper { text.to_uppercase() } else { text }
}

fn convert(spec: &Spec, arguments: &mut Arguments) -> Result<String, Stop> {
    let text = match spec.conversion {
        'd' | 'i' => {
            let value = arguments.integer();
            let digits = match spec.precision {
                Some(precision) => format!("{:0precision$}", value.unsigned_abs()),
                None => value.unsigned_abs().to_string(),
            };
            let digits = if value < 0 {
                format!("-{digits}")
            } else {
                digits
            };
            pad(
                spec,
                signed(spec, digits, value < 0),
                spec.precision.is_none(),
            )
        }
        'u' | 'o' | 'x' | 'X' => {
            let value = arguments.integer() as u64;
            let mut body = match spec.conversion {
                'o' => format!("{value:o}"),
                'x' => format!("{value:x}"),
                'X' => format!("{value:X}"),
                _ => value.to_string(),
            };
            if let Some(precision) = spec.precision {
                body = format!("{body:0>precision$}");
            }
            if spec.alternate && value != 0 {
                body = match spec.conversion {
                    'o' if !body.starts_with('0') => format!("0{body}"),
                    'x' => format!("0x{body}"),
                    'X' => format!("0X{body}"),
                    _ => body,
                };
            }
            pad(spec, body, spec.precision.is_none())
        }
        'f' | 'F' | 'e' | 'E' | 'g' | 'G' | 'a' | 'A' => {
            let value = arguments.float();
            let precision = spec.precision.unwrap_or(6);
            let body = match spec.conversion {
                'f' | 'F' => format!("{value:.precision$}"),
                'e' => c_exponent(value, precision, false),
                'E' => c_exponent(value, precision, true),
                'a' => hex_float(value, false),
                'A' => hex_float(value, true),
                _ => general(value, precision, spec.conversion == 'G'),
            };
            pad(spec, signed(spec, body, value.is_sign_negative()), true)
        }
        'c' => {
            let body = arguments
                .next()
                .and_then(|text| text.chars().next())
                .map(String::from)
                .unwrap_or_default();
            pad(spec, body, false)
        }
        's' => {
            let mut body = arguments.next().unwrap_or_default().to_string();
            if let Some(precision) = spec.precision {
                body = body.chars().take(precision).collect();
            }
            pad(spec, body, false)
        }
        'b' => {
            let mut body = String::new();
            let keep_going = escape(arguments.next().unwrap_or_default(), &mut body, false);
            if let Some(precision) = spec.precision {
                body = body.chars().take(precision).collect();
            }
            let padded = pad(spec, body, false);
            if !keep_going {
                return Err(Stop::Escape(padded));
            }
            padded
        }
        'q' | 'Q' => {
            let mut text = arguments.next().unwrap_or_default().to_string();
            if spec.conversion == 'Q'
                && let Some(precision) = spec.precision
            {
                text = text.chars().take(precision).collect();
            }
            pad(spec, shell_quote(&text), false)
        }
        'T' => {
            let seconds = time_argument(arguments);
            let mut body = format_time(spec.time_format.as_deref().unwrap_or(""), seconds);
            if let Some(precision) = spec.precision {
                body = body.chars().take(precision).collect();
            }
            pad(spec, body, false)
        }
        '\0' => {
            arguments.failed = true;
            return Err(Stop::Error);
        }
        other => {
            shell::error(format!("printf: `{other}': invalid format character"));
            arguments.failed = true;
            return Err(Stop::Error);
        }
    };
    Ok(text)
}

fn c_exponent(value: f64, precision: usize, upper: bool) -> String {
    let text = format!("{value:.precision$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let sign = if exponent < 0 { '-' } else { '+' };
    let marker = if upper { 'E' } else { 'e' };
    format!("{mantissa}{marker}{sign}{:02}", exponent.abs())
}

fn general(value: f64, precision: usize, upper: bool) -> String {
    let precision = precision.max(1);
    if value == 0.0 {
        return "0".into();
    }
    if value.is_nan() {
        return if upper { "NAN" } else { "nan" }.into();
    }
    if value.is_infinite() {
        let text = if value < 0.0 { "-inf" } else { "inf" };
        return if upper {
            text.to_uppercase()
        } else {
            text.into()
        };
    }
    let exponent = value.abs().log10().floor() as i32;
    if exponent < -4 || exponent >= precision as i32 {
        let text = c_exponent(value, precision - 1, upper);
        let (mantissa, rest) = text.split_at(text.find(['e', 'E']).unwrap_or(text.len()));
        let mantissa = if mantissa.contains('.') {
            mantissa.trim_end_matches('0').trim_end_matches('.')
        } else {
            mantissa
        };
        format!("{mantissa}{rest}")
    } else {
        let decimals = (precision as i32 - 1 - exponent).max(0) as usize;
        let text = format!("{value:.decimals$}");
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            text
        }
    }
}

fn literal_number(chars: &[char], index: &mut usize) -> Result<Option<usize>, ()> {
    let start = *index;
    while chars.get(*index).is_some_and(char::is_ascii_digit) {
        *index += 1;
    }
    if start == *index {
        return Ok(None);
    }
    let digits: String = chars[start..*index].iter().collect();
    match digits.parse::<u64>() {
        Ok(value) if value <= i32::MAX as u64 => Ok(Some(value as usize)),
        _ => Err(()),
    }
}

fn parse_spec(chars: &[char], index: &mut usize, arguments: &mut Arguments) -> Result<Spec, Stop> {
    let start = *index;
    let mut spec = Spec {
        left: false,
        plus: false,
        space: false,
        zero: false,
        alternate: false,
        width: None,
        precision: None,
        time_format: None,
        conversion: '\0',
    };
    while let Some(&flag) = chars.get(*index) {
        match flag {
            '-' => spec.left = true,
            '+' => spec.plus = true,
            ' ' => spec.space = true,
            '0' => spec.zero = true,
            '#' => spec.alternate = true,
            _ => break,
        }
        *index += 1;
    }
    let too_large = || {
        shell::error("printf: Value too large to be stored in data type");
        Stop::Error
    };
    if chars.get(*index) == Some(&'*') {
        *index += 1;
        if let Some(width) = arguments.star() {
            spec.left |= width < 0;
            spec.width = Some(width.unsigned_abs() as usize);
        }
    } else {
        spec.width = literal_number(chars, index).map_err(|_| too_large())?;
    }
    if chars.get(*index) == Some(&'.') {
        *index += 1;
        if chars.get(*index) == Some(&'*') {
            *index += 1;
            spec.precision = arguments
                .star()
                .and_then(|precision| usize::try_from(precision).ok());
        } else {
            spec.precision = Some(
                literal_number(chars, index)
                    .unwrap_or(Some(i32::MAX as usize))
                    .unwrap_or(0),
            );
        }
    }
    while chars
        .get(*index)
        .is_some_and(|modifier| LENGTH_MODIFIERS.contains(*modifier))
    {
        *index += 1;
    }
    if chars.get(*index) == Some(&'(') {
        let close = chars[*index..]
            .iter()
            .position(|character| *character == ')');
        if let Some(close) = close {
            spec.time_format = Some(chars[*index + 1..*index + close].iter().collect());
            *index += close + 1;
        }
    }
    match chars.get(*index) {
        Some(&conversion) => {
            spec.conversion = conversion;
            *index += 1;
        }
        None => {
            let text: String = chars[start - 1..].iter().collect();
            shell::error(format!("printf: `{text}': missing format character"));
            arguments.failed = true;
            return Err(Stop::Error);
        }
    }
    if spec.time_format.is_some() && spec.conversion != 'T' {
        shell::error(format!(
            "printf: `{}': invalid format character",
            spec.conversion
        ));
        arguments.failed = true;
        return Err(Stop::Error);
    }
    Ok(spec)
}

#[cfg(test)]
fn format(template: &str, values: &[String]) -> (String, bool) {
    format_with(template, values, 0)
}

fn format_with(template: &str, values: &[String], shell_started: i64) -> (String, bool) {
    let mut out = String::new();
    let mut arguments = Arguments {
        values,
        position: 0,
        failed: false,
        shell_started,
    };
    let chars: Vec<char> = template.chars().collect();
    loop {
        let start_position = arguments.position;
        let mut literal = String::new();
        let mut index = 0;
        while index < chars.len() {
            match chars[index] {
                '%' if chars.get(index + 1) == Some(&'%') => {
                    literal.push('%');
                    index += 2;
                }
                '%' => {
                    escape(&std::mem::take(&mut literal), &mut out, true);
                    index += 1;
                    let converted = parse_spec(&chars, &mut index, &mut arguments)
                        .and_then(|spec| convert(&spec, &mut arguments));
                    match converted {
                        Ok(text) => out.push_str(&text),
                        Err(Stop::Escape(text)) => {
                            out.push_str(&text);
                            return (out, arguments.failed);
                        }
                        Err(Stop::Error) => return (out, true),
                    }
                }
                character => {
                    literal.push(character);
                    index += 1;
                }
            }
        }
        escape(&literal, &mut out, true);
        if arguments.position >= values.len() || arguments.position == start_position {
            break;
        }
    }
    (out, arguments.failed)
}

fn assign_target(shell: &mut Shell, target: &str, text: &str) -> Result<(), String> {
    let Some((name, rest)) = target.split_once('[') else {
        shell.try_set_variable(target, text)?;
        return Ok(());
    };
    let subscript = rest.strip_suffix(']').unwrap_or(rest);
    let key = if shell.is_associative(name) {
        Key::Name(subscript.to_string())
    } else {
        Key::Index(crate::expand::evaluate_arithmetic(shell, subscript)?)
    };
    shell.set_element(name, key, text)
}

fn valid_target(target: &str) -> bool {
    match target.split_once('[') {
        Some((name, rest)) => is_name(name) && rest.ends_with(']'),
        None => is_name(target),
    }
}

pub(super) fn run(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let mut args = args;
    let mut target = None;
    loop {
        match args {
            [flag, rest @ ..] if flag == "--" => {
                args = rest;
                break;
            }
            [flag] if flag == "-v" => {
                shell::error("printf: -v: option requires an argument");
                eprintln!("{USAGE}");
                return 2;
            }
            [flag, name, rest @ ..] if flag == "-v" => {
                target = Some(name.clone());
                args = rest;
            }
            _ => break,
        }
    }
    if let Some(name) = &target
        && !valid_target(name)
    {
        shell::error(format!("printf: `{name}': not a valid identifier"));
        return 2;
    }
    let Some((template, values)) = args.split_first() else {
        eprintln!("{USAGE}");
        return 2;
    };
    let shell_started = std::time::SystemTime::now()
        .checked_sub(shell.started.elapsed())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs() as i64);
    let (text, failed) = format_with(template, values, shell_started);
    let text = bytes::canonical(&text);
    match target {
        Some(name) => {
            if let Err(message) = assign_target(shell, &name, &text) {
                shell::error(format!("printf: {message}"));
                return 1;
            }
        }
        None => {
            let _ = write!(out, "{text}");
        }
    }
    i32::from(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printf(template: &str, values: &[&str]) -> String {
        let values: Vec<String> = values.iter().map(|value| value.to_string()).collect();
        format(template, &values).0
    }

    #[test]
    fn strings_and_padding() {
        assert_eq!(printf("[%s]\\n", &["hi"]), "[hi]\n");
        assert_eq!(printf("[%5s][%-5s]", &["a", "b"]), "[    a][b    ]");
        assert_eq!(printf("[%.2s]", &["hello"]), "[he]");
    }

    #[test]
    fn numbers() {
        assert_eq!(
            printf(
                "%d %05d %+d %x %X %o %#x",
                &["42", "-42", "7", "255", "255", "8", "255"]
            ),
            "42 -0042 +7 ff FF 10 0xff"
        );
        assert_eq!(
            printf("%.2f %8.3f %e", &["3.14159", "2.5", "12345.678"]),
            "3.14    2.500 1.234568e+04"
        );
        assert_eq!(printf("%g %g", &["0.0001", "100000"]), "0.0001 100000");
        assert_eq!(printf("%d", &["'A"]), "65");
        assert_eq!(printf("%ld %a", &["3", "1"]), "3 0x1p+0");
    }

    #[test]
    fn format_is_reused_for_extra_arguments() {
        assert_eq!(printf("%s=%s\\n", &["a", "1", "b", "2"]), "a=1\nb=2\n");
        assert_eq!(printf("%s,", &[]), ",");
    }

    #[test]
    fn escapes_and_special_conversions() {
        assert_eq!(printf("%b", &["a\\tb"]), "a\tb");
        assert_eq!(printf("%q", &["it's here"]), "it\\'s\\ here");
        assert_eq!(printf("%q %q", &["", "a\tb"]), "'' $'a\\tb'");
        assert_eq!(printf("100%%\\x41\\101", &[]), "100%AA");
        assert_eq!(printf("%*d|", &["4", "7"]), "   7|");
        assert_eq!(printf("[%*d][%.*s]", &["-3", "7", "-1", "ab"]), "[7  ][ab]");
    }

    #[test]
    fn invalid_numbers_keep_the_valid_prefix() {
        let (text, failed) = format("%d %d", &["3.7".to_string(), "abc".to_string()]);
        assert_eq!(text, "3 0");
        assert!(failed);
        assert_eq!(integer_prefix("99999999999999999999").0, i64::MAX);
    }

    #[test]
    fn huge_widths_are_refused() {
        let (text, failed) = format("%*d", &["99999999999".to_string(), "1".to_string()]);
        assert_eq!(text, "1");
        assert!(failed);
        let (text, failed) = format("x%99999999999d", &["1".to_string()]);
        assert_eq!(text, "x");
        assert!(failed);
    }

    #[test]
    fn time_conversion() {
        assert_eq!(format_time("%s", 86400), "86400");
        assert_eq!(printf("%(%s)T", &["3600"]), "3600");
    }
}
