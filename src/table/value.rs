use std::cmp::Ordering;

use super::datetime;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Nothing,
    Bool(bool),
    Int(i64),
    Float(f64),
    Size(u64),
    Date(i64),
    Text(String),
}

const SIZE_UNITS: &[(&str, u64)] = &[
    ("b", 1),
    ("k", 1 << 10),
    ("kb", 1 << 10),
    ("kib", 1 << 10),
    ("m", 1 << 20),
    ("mb", 1 << 20),
    ("mib", 1 << 20),
    ("g", 1 << 30),
    ("gb", 1 << 30),
    ("gib", 1 << 30),
    ("t", 1 << 40),
    ("tb", 1 << 40),
    ("tib", 1 << 40),
];

pub(crate) fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    let split = text
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let unit = unit.trim();
    let multiplier = if unit.is_empty() {
        1
    } else {
        SIZE_UNITS
            .iter()
            .find(|(name, _)| *name == unit)
            .map(|(_, multiplier)| *multiplier)?
    };
    Some((number * multiplier as f64) as u64)
}

pub(crate) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut amount = bytes as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit + 1 < UNITS.len() {
        amount /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if amount < 10.0 {
        format!("{amount:.1} {}", UNITS[unit])
    } else {
        format!("{amount:.0} {}", UNITS[unit])
    }
}

impl Value {
    pub(crate) fn infer(text: &str) -> Value {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Value::Nothing;
        }
        if let Ok(number) = trimmed.parse::<i64>() {
            return Value::Int(number);
        }
        if trimmed.contains('.')
            && let Ok(number) = trimmed.parse::<f64>()
        {
            return Value::Float(number);
        }
        match trimmed {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::Text(text.to_string()),
        }
    }

    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_) | Value::Size(_))
    }

    pub(crate) fn as_number(&self) -> Option<f64> {
        match self {
            Value::Int(number) => Some(*number as f64),
            Value::Float(number) => Some(*number),
            Value::Size(bytes) => Some(*bytes as f64),
            Value::Date(timestamp) => Some(*timestamp as f64),
            Value::Bool(flag) => Some(f64::from(u8::from(*flag))),
            _ => None,
        }
    }

    pub(crate) fn display(&self, now: i64) -> String {
        match self {
            Value::Size(bytes) => format_size(*bytes),
            Value::Date(timestamp) => datetime::format_relative(*timestamp, now),
            other => other.raw(),
        }
    }

    pub(crate) fn plain(&self) -> String {
        match self {
            Value::Size(bytes) => format_size(*bytes),
            Value::Date(timestamp) => datetime::format_local(*timestamp),
            other => other.raw(),
        }
    }

    pub(crate) fn raw(&self) -> String {
        match self {
            Value::Nothing => String::new(),
            Value::Bool(flag) => flag.to_string(),
            Value::Int(number) => number.to_string(),
            Value::Float(number) => format_float(*number),
            Value::Size(bytes) => bytes.to_string(),
            Value::Date(timestamp) => datetime::format_local(*timestamp),
            Value::Text(text) => text.clone(),
        }
    }

    pub(crate) fn literal_like(&self, text: &str) -> Value {
        match self {
            Value::Size(_) => parse_size(text).map_or_else(|| Value::infer(text), Value::Size),
            Value::Date(_) => datetime::parse_date(text)
                .or_else(|| datetime::parse_age(text).map(|age| datetime::now() - age))
                .map_or_else(|| Value::infer(text), Value::Date),
            Value::Int(_) | Value::Float(_) => match Value::infer(text) {
                Value::Text(_) => parse_size(text).map_or_else(
                    || Value::Text(text.to_string()),
                    |bytes| Value::Int(bytes as i64),
                ),
                other => other,
            },
            Value::Text(_) => Value::Text(text.to_string()),
            _ => Value::infer(text),
        }
    }

    pub(crate) fn compare(&self, other: &Value) -> Option<Ordering> {
        match (self, other) {
            (Value::Nothing, Value::Nothing) => Some(Ordering::Equal),
            (Value::Nothing, _) | (_, Value::Nothing) => None,
            (Value::Text(left), Value::Text(right)) => Some(left.cmp(right)),
            (Value::Text(_), _) | (_, Value::Text(_)) => Some(self.raw().cmp(&other.raw())),
            _ => self.as_number()?.partial_cmp(&other.as_number()?),
        }
    }

    pub(crate) fn sort_cmp(&self, other: &Value) -> Ordering {
        match (self, other) {
            (Value::Nothing, Value::Nothing) => Ordering::Equal,
            (Value::Nothing, _) => Ordering::Greater,
            (_, Value::Nothing) => Ordering::Less,
            _ => self.compare(other).unwrap_or(Ordering::Equal),
        }
    }
}

fn format_float(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{number:.1}")
    } else {
        let text = format!("{number:.6}");
        text.trim_end_matches('0').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_parse_with_binary_units() {
        assert_eq!(parse_size("512"), Some(512));
        assert_eq!(parse_size("1kb"), Some(1024));
        assert_eq!(parse_size("1.5 MB"), Some(1_572_864));
        assert_eq!(parse_size("2G"), Some(2 << 30));
        assert_eq!(parse_size("lots"), None);
    }

    #[test]
    fn sizes_format_compactly() {
        assert_eq!(format_size(900), "900 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(52 << 20), "52 MB");
    }

    #[test]
    fn inference() {
        assert_eq!(Value::infer("42"), Value::Int(42));
        assert_eq!(Value::infer("4.5"), Value::Float(4.5));
        assert_eq!(Value::infer("true"), Value::Bool(true));
        assert_eq!(Value::infer(""), Value::Nothing);
        assert_eq!(Value::infer("abc"), Value::Text("abc".into()));
    }

    #[test]
    fn literals_follow_the_column_type() {
        assert_eq!(Value::Size(0).literal_like("1kb"), Value::Size(1024));
        assert_eq!(Value::Int(0).literal_like("2k"), Value::Int(2048));
        assert_eq!(
            Value::Text(String::new()).literal_like("12"),
            Value::Text("12".into())
        );
        assert!(matches!(Value::Date(0).literal_like("1d"), Value::Date(_)));
    }

    #[test]
    fn comparisons_across_numeric_kinds() {
        assert_eq!(
            Value::Int(3).compare(&Value::Float(2.5)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Value::Size(10).compare(&Value::Int(10)),
            Some(Ordering::Equal)
        );
        assert_eq!(Value::Nothing.compare(&Value::Int(1)), None);
        assert_eq!(Value::Nothing.sort_cmp(&Value::Int(1)), Ordering::Greater);
    }

    #[test]
    fn floats_keep_a_decimal_point() {
        assert_eq!(Value::Float(3.0).raw(), "3.0");
        assert_eq!(Value::Float(0.25).raw(), "0.25");
    }
}
