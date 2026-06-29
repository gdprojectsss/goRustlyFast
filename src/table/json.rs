#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

pub(crate) fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        position: 0,
    };
    parser.skip_whitespace();
    let value = parser.value()?;
    parser.skip_whitespace();
    if parser.position < parser.chars.len() {
        return Err(parser.error("trailing characters"));
    }
    Ok(value)
}

pub(crate) fn parse_lines(text: &str) -> Result<Vec<Json>, String> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse)
        .collect()
}

struct Parser {
    chars: Vec<char>,
    position: usize,
}

impl Parser {
    fn error(&self, message: &str) -> String {
        format!("invalid JSON at character {}: {message}", self.position + 1)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.position += 1;
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.peek() == Some(expected) {
            self.position += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{expected}'")))
        }
    }

    fn keyword(&mut self, word: &str, value: Json) -> Result<Json, String> {
        let end = self.position + word.len();
        if end <= self.chars.len()
            && self.chars[self.position..end]
                .iter()
                .copied()
                .eq(word.chars())
        {
            self.position = end;
            Ok(value)
        } else {
            Err(self.error("unexpected word"))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => self.string().map(Json::String),
            Some('t') => self.keyword("true", Json::Bool(true)),
            Some('f') => self.keyword("false", Json::Bool(false)),
            Some('n') => self.keyword("null", Json::Null),
            Some(character) if character == '-' || character.is_ascii_digit() => self.number(),
            Some(_) => Err(self.error("unexpected character")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect('{')?;
        let mut fields = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some('}') {
            self.position += 1;
            return Ok(Json::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(':')?;
            self.skip_whitespace();
            fields.push((key, self.value()?));
            self.skip_whitespace();
            match self.peek() {
                Some(',') => self.position += 1,
                Some('}') => {
                    self.position += 1;
                    return Ok(Json::Object(fields));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.position += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => self.position += 1,
                Some(']') => {
                    self.position += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|character| character.is_ascii_digit() || "+-.eE".contains(character))
        {
            self.position += 1;
        }
        let text: String = self.chars[start..self.position].iter().collect();
        if text.parse::<f64>().is_err() {
            return Err(self.error("invalid number"));
        }
        Ok(Json::Number(text))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            let Some(character) = self.peek() else {
                return Err(self.error("unterminated string"));
            };
            self.position += 1;
            match character {
                '"' => return Ok(out),
                '\\' => {
                    let Some(escaped) = self.peek() else {
                        return Err(self.error("unterminated escape"));
                    };
                    self.position += 1;
                    match escaped {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => out.push(self.unicode_escape()?),
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let end = self.position + 4;
        if end > self.chars.len() {
            return Err(self.error("short unicode escape"));
        }
        let digits: String = self.chars[self.position..end].iter().collect();
        self.position = end;
        u32::from_str_radix(&digits, 16).map_err(|_| self.error("bad unicode escape"))
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let high = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&high)
            && self.chars.get(self.position) == Some(&'\\')
            && self.chars.get(self.position + 1) == Some(&'u')
        {
            self.position += 2;
            let low = self.hex4()?;
            0x10000 + ((high - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF)
        } else {
            high
        };
        Ok(char::from_u32(code).unwrap_or('\u{FFFD}'))
    }
}

pub(crate) fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

impl Json {
    pub(crate) fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
            Json::Number(text) => out.push_str(text),
            Json::String(text) => write_string(out, text),
            Json::Array(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Json::Object(fields) => {
                out.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(out, key);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }

    pub(crate) fn to_text(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let text = r#"[{"name":"a\"b","n":-1.5e3,"ok":true,"none":null,"list":[1,2]}]"#;
        let parsed = parse(text).unwrap();
        assert_eq!(parsed.to_text(), text);
    }

    #[test]
    fn whitespace_and_unicode() {
        let parsed = parse(" { \"k\" : \"\\u00e9\\ud83d\\ude00\" } ").unwrap();
        assert_eq!(
            parsed,
            Json::Object(vec![("k".into(), Json::String("\u{e9}\u{1F600}".into()))])
        );
    }

    #[test]
    fn errors_point_at_the_problem() {
        assert!(parse("[1,]").unwrap_err().contains("character 4"));
        assert!(parse("{\"a\" 1}").is_err());
        assert!(parse("tru").is_err());
        assert!(parse("[1] x").is_err());
    }

    #[test]
    fn json_lines() {
        assert_eq!(parse_lines("{\"a\":1}\n\n{\"a\":2}\n").unwrap().len(), 2);
    }
}
