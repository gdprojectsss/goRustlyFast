use std::io::Write;

use crate::bytes;
use crate::escapes;
use crate::shell::{self, Shell};

struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    zero: bool,
    alternate: bool,
    width: Option<usize>,
    precision: Option<usize>,
    conversion: char,
}

struct Arguments<'a> {
    values: &'a [String],
    position: usize,
    failed: bool,
    stopped: bool,
}

impl<'a> Arguments<'a> {
    fn next(&mut self) -> Option<&'a str> {
        let value = self.values.get(self.position).map(String::as_str);
        if value.is_some() {
            self.position += 1;
        }
        value
    }

    fn integer(&mut self) -> i64 {
        let Some(text) = self.next() else {
            return 0;
        };
        let text = text.trim();
        if let Some(quoted) = text.strip_prefix(['\'', '"']) {
            return quoted.chars().next().map_or(0, |character| {
                bytes::raw_byte(character).map_or(character as i64, i64::from)
            });
        }
        let parsed = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
        {
            i64::from_str_radix(hex, 16)
        } else if text.len() > 1 && text.starts_with('0') {
            i64::from_str_radix(&text[1..], 8)
        } else {
            text.parse()
        };
        parsed.unwrap_or_else(|_| {
            shell::error(format!("printf: {text}: invalid number"));
            self.failed = true;
            0
        })
    }

    fn float(&mut self) -> f64 {
        let Some(text) = self.next() else {
            return 0.0;
        };
        text.trim().parse().unwrap_or_else(|_| {
            shell::error(format!("printf: {text}: invalid number"));
            self.failed = true;
            0.0
        })
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
    if escapes::needs_ansi_quote(text) {
        return escapes::ansi_quote(text);
    }
    if !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_alphanumeric() || "-_./=:@%+,".contains(character))
    {
        return text.to_string();
    }
    format!("'{}'", text.replace('\'', "'\\''"))
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

fn convert(spec: &Spec, arguments: &mut Arguments) -> String {
    match spec.conversion {
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
            if spec.alternate && value != 0 {
                body = match spec.conversion {
                    'o' => format!("0{body}"),
                    'x' => format!("0x{body}"),
                    'X' => format!("0X{body}"),
                    _ => body,
                };
            }
            pad(spec, body, true)
        }
        'f' | 'F' | 'e' | 'E' | 'g' | 'G' => {
            let value = arguments.float();
            let precision = spec.precision.unwrap_or(6);
            let body = match spec.conversion {
                'f' | 'F' => format!("{value:.precision$}"),
                'e' => c_exponent(value, precision, false),
                'E' => c_exponent(value, precision, true),
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
            arguments.stopped = !escape(arguments.next().unwrap_or_default(), &mut body, false);
            if let Some(precision) = spec.precision {
                body = body.chars().take(precision).collect();
            }
            pad(spec, body, false)
        }
        'q' => pad(
            spec,
            shell_quote(arguments.next().unwrap_or_default()),
            false,
        ),
        other => {
            shell::error(format!("printf: %{other}: invalid conversion"));
            arguments.failed = true;
            String::new()
        }
    }
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

fn parse_spec(chars: &[char], index: &mut usize, arguments: &mut Arguments) -> Spec {
    let mut spec = Spec {
        left: false,
        plus: false,
        space: false,
        zero: false,
        alternate: false,
        width: None,
        precision: None,
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
    let number = |index: &mut usize, arguments: &mut Arguments| -> Option<usize> {
        if chars.get(*index) == Some(&'*') {
            *index += 1;
            let value = arguments.integer();
            return Some(value.unsigned_abs() as usize);
        }
        let start = *index;
        while chars.get(*index).is_some_and(char::is_ascii_digit) {
            *index += 1;
        }
        (start != *index).then(|| {
            chars[start..*index]
                .iter()
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
    };
    spec.width = number(index, arguments);
    if chars.get(*index) == Some(&'.') {
        *index += 1;
        spec.precision = Some(number(index, arguments).unwrap_or(0));
    }
    spec.conversion = chars.get(*index).copied().unwrap_or('\0');
    *index += 1;
    spec
}

fn format(template: &str, values: &[String]) -> (String, bool) {
    let mut out = String::new();
    let mut arguments = Arguments {
        values,
        position: 0,
        failed: false,
        stopped: false,
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
                    let spec = parse_spec(&chars, &mut index, &mut arguments);
                    out.push_str(&convert(&spec, &mut arguments));
                    if arguments.stopped {
                        return (out, arguments.failed);
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

pub(super) fn run(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let (target, args) = match args {
        [flag, name, rest @ ..] if flag == "-v" => (Some(name.clone()), rest),
        _ => (None, args),
    };
    let Some((template, values)) = args.split_first() else {
        shell::error("printf: usage: printf [-v var] format [arguments]");
        return 2;
    };
    let (text, failed) = format(template, values);
    let text = bytes::canonical(&text);
    match target {
        Some(name) => shell.set_variable(&name, &text),
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
    }

    #[test]
    fn format_is_reused_for_extra_arguments() {
        assert_eq!(printf("%s=%s\\n", &["a", "1", "b", "2"]), "a=1\nb=2\n");
        assert_eq!(printf("%s,", &[]), ",");
    }

    #[test]
    fn escapes_and_special_conversions() {
        assert_eq!(printf("%b", &["a\\tb"]), "a\tb");
        assert_eq!(printf("%q", &["it's here"]), "'it'\\''s here'");
        assert_eq!(printf("100%%\\x41\\101", &[]), "100%AA");
        assert_eq!(printf("%*d|", &["4", "7"]), "   7|");
    }

    #[test]
    fn invalid_numbers_are_reported() {
        let (text, failed) = format("%d", &["abc".to_string()]);
        assert_eq!(text, "0");
        assert!(failed);
    }
}
