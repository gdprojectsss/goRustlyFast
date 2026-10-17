use crate::shell::Shell;
use crate::variables::Key;

const MAX_RECURSION: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64),
    Name(String, Option<String>),
    Operator(&'static str),
}

const OPERATORS: &[&str] = &[
    "<<=", ">>=", "**", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "++", "--", "+=", "-=",
    "*=", "/=", "%=", "&=", "|=", "^=", "+", "-", "*", "/", "%", "<", ">", "&", "|", "^", "!", "~",
    "=", "(", ")", "?", ":", ",",
];

const ASSIGNMENTS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "<<=", ">>=", "&=", "|=", "^=",
];

fn digit_value(character: char, base: u32) -> Option<u32> {
    let value = match character {
        '0'..='9' => character as u32 - '0' as u32,
        'a'..='z' => character as u32 - 'a' as u32 + 10,
        'A'..='Z' if base <= 36 => character as u32 - 'A' as u32 + 10,
        'A'..='Z' => character as u32 - 'A' as u32 + 36,
        '@' => 62,
        '_' => 63,
        _ => return None,
    };
    Some(value)
}

fn digits_in_base(digits: &str, base: u32) -> Result<i64, String> {
    if digits.is_empty() {
        return Err("invalid number".into());
    }
    digits.chars().try_fold(0i64, |total, character| {
        match digit_value(character, base) {
            Some(digit) if digit < base => Ok(total
                .wrapping_mul(i64::from(base))
                .wrapping_add(i64::from(digit))),
            Some(_) => Err("value too great for base".into()),
            None => Err("invalid number".into()),
        }
    })
}

fn parse_number(text: &str) -> Result<i64, String> {
    if let Some((base, digits)) = text.split_once('#') {
        let base: u32 = base
            .parse()
            .ok()
            .filter(|base| (2..=64).contains(base))
            .ok_or_else(|| "invalid arithmetic base".to_string())?;
        return digits_in_base(digits, base);
    }
    if let Some(hex_digits) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return digits_in_base(hex_digits, 16);
    }
    if text.len() > 1 && text.starts_with('0') {
        return digits_in_base(&text[1..], 8);
    }
    digits_in_base(text, 10)
}

fn subscript_end(text: &str) -> Option<usize> {
    let mut depth = 0;
    for (index, character) in text.char_indices() {
        match character {
            '[' => depth += 1,
            ']' if depth == 1 => return Some(index),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

struct Lexed {
    tokens: Vec<Token>,
    offsets: Vec<usize>,
}

fn tokenize(source: &str) -> Result<Lexed, String> {
    let mut tokens = Vec::new();
    let mut offsets = Vec::new();
    let skippable = |character: char| character.is_whitespace() || character == '"';
    let mut rest = source.trim_start_matches(skippable);
    while !rest.is_empty() {
        let offset = source.len() - rest.len();
        let character = rest.chars().next().unwrap_or(' ');
        if character.is_ascii_digit() {
            let end = rest
                .find(|character: char| {
                    !(character.is_ascii_alphanumeric() || "#@_".contains(character))
                })
                .unwrap_or(rest.len());
            let text = &rest[..end];
            let number = parse_number(text)
                .map_err(|message| format!("{text}: {message} (error token is \"{text}\")"))?;
            tokens.push(Token::Number(number));
            rest = &rest[end..];
        } else if character == '_' || character.is_ascii_alphabetic() {
            let end = rest
                .find(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))
                .unwrap_or(rest.len());
            let name = rest[..end].to_string();
            rest = &rest[end..];
            let subscript = match rest.starts_with('[').then(|| subscript_end(rest)) {
                Some(Some(close)) => {
                    let inside = rest[1..close].to_string();
                    rest = &rest[close + 1..];
                    Some(inside)
                }
                Some(None) => return Err(format!("{source}: bad array subscript")),
                None => None,
            };
            tokens.push(Token::Name(name, subscript));
        } else if let Some(operator) = OPERATORS
            .iter()
            .find(|operator| rest.starts_with(**operator))
        {
            tokens.push(Token::Operator(operator));
            rest = &rest[operator.len()..];
        } else {
            return Err(format!(
                "{source}: arithmetic syntax error: operand expected (error token is \"{rest}\")"
            ));
        }
        offsets.push(offset);
        rest = rest.trim_start_matches(skippable);
    }
    Ok(Lexed { tokens, offsets })
}

pub fn eval(shell: &mut Shell, source: &str) -> Result<i64, String> {
    eval_nested(shell, source, 0)
}

fn eval_nested(shell: &mut Shell, source: &str, depth: usize) -> Result<i64, String> {
    let lexed = tokenize(source)?;
    if lexed.tokens.is_empty() {
        return Ok(0);
    }
    let mut evaluator = Evaluator {
        tokens: lexed.tokens,
        offsets: lexed.offsets,
        source,
        position: 0,
        skipping: 0,
        depth,
        shell,
    };
    let value = evaluator.comma()?;
    if evaluator.position < evaluator.tokens.len() {
        return Err(evaluator.error("arithmetic syntax error in expression"));
    }
    Ok(value)
}

struct Evaluator<'a> {
    tokens: Vec<Token>,
    offsets: Vec<usize>,
    source: &'a str,
    position: usize,
    skipping: usize,
    depth: usize,
    shell: &'a mut Shell,
}

fn binary_precedence(operator: &str) -> Option<u8> {
    Some(match operator {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | ">" | "<=" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        "**" => 11,
        _ => return None,
    })
}

fn power(base: i64, exponent: i64) -> i64 {
    let mut result: i64 = 1;
    let mut base = base;
    let mut exponent = exponent as u64;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result.wrapping_mul(base);
        }
        base = base.wrapping_mul(base);
        exponent >>= 1;
    }
    result
}

fn apply(operator: &str, left: i64, right: i64) -> Result<i64, &'static str> {
    Ok(match operator {
        "|" => left | right,
        "^" => left ^ right,
        "&" => left & right,
        "==" => (left == right) as i64,
        "!=" => (left != right) as i64,
        "<" => (left < right) as i64,
        ">" => (left > right) as i64,
        "<=" => (left <= right) as i64,
        ">=" => (left >= right) as i64,
        "<<" => left.wrapping_shl(right as u32),
        ">>" => left.wrapping_shr(right as u32),
        "+" => left.wrapping_add(right),
        "-" => left.wrapping_sub(right),
        "*" => left.wrapping_mul(right),
        "/" | "%" if right == 0 => return Err("division by 0"),
        "/" => left.wrapping_div(right),
        "%" => left.wrapping_rem(right),
        "**" if right < 0 => return Err("exponent less than 0"),
        "**" => power(left, right),
        _ => return Err("arithmetic syntax error in expression"),
    })
}

impl Evaluator<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn peek_operator(&self) -> Option<&'static str> {
        match self.peek() {
            Some(Token::Operator(operator)) => Some(operator),
            _ => None,
        }
    }

    fn rest_from(&self, position: usize) -> &str {
        let offset = self
            .offsets
            .get(position)
            .copied()
            .unwrap_or(self.source.len());
        &self.source[offset..]
    }

    fn error_at(&self, position: usize, message: &str) -> String {
        format!(
            "{}: {message} (error token is \"{}\")",
            self.source.trim_start(),
            self.rest_from(position)
        )
    }

    fn split_increment(&mut self) {
        let sign = match self.peek_operator() {
            Some("++") => "+",
            Some("--") => "-",
            _ => return,
        };
        let offset = self.offsets[self.position];
        self.tokens[self.position] = Token::Operator(sign);
        self.tokens.insert(self.position + 1, Token::Operator(sign));
        self.offsets.insert(self.position + 1, offset + 1);
    }

    fn error(&self, message: &str) -> String {
        self.error_at(self.position, message)
    }

    fn expect(&mut self, operator: &str, message: &str) -> Result<(), String> {
        if self.peek_operator() == Some(operator) {
            self.position += 1;
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn key(&mut self, name: &str, subscript: &str) -> Result<Key, String> {
        if self.shell.is_associative(name) {
            return Ok(Key::Name(subscript.trim().to_string()));
        }
        let index = if self.skipping > 0 {
            0
        } else {
            eval_nested(self.shell, subscript, self.depth + 1)?
        };
        Ok(Key::Index(index))
    }

    fn raw_value(&mut self, name: &str, subscript: Option<&str>) -> Result<String, String> {
        let subscript = match subscript {
            Some(subscript) => subscript,
            None if self.shell.arrays.contains_key(name) => "0",
            None => return Ok(self.shell.get_variable(name).unwrap_or_default()),
        };
        let key = self.key(name, subscript)?;
        if let Some(values) = self.shell.special_array(name) {
            let Key::Index(index) = key else {
                return Ok(String::new());
            };
            return Ok(usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index).cloned())
                .unwrap_or_default());
        }
        Ok(match self.shell.arrays.get(name) {
            Some(array) => array.get(&key).cloned().unwrap_or_default(),
            None if key == Key::Index(0) => self.shell.get_variable(name).unwrap_or_default(),
            None => String::new(),
        })
    }

    fn value(&mut self, name: &str, subscript: Option<&str>) -> Result<i64, String> {
        if self.skipping > 0 {
            return Ok(0);
        }
        let text = self.raw_value(name, subscript)?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(0);
        }
        if let Ok(number) = trimmed.parse::<i64>()
            && !trimmed.starts_with(['+', '0'])
        {
            return Ok(number);
        }
        if self.depth >= MAX_RECURSION || crate::stack::exhausted() {
            return Err(format!(
                "{name}: expression recursion level exceeded (error token is \"{name}\")"
            ));
        }
        eval_nested(self.shell, trimmed, self.depth + 1)
    }

    fn store(&mut self, name: &str, subscript: Option<&str>, value: i64) -> Result<(), String> {
        if self.skipping > 0 {
            return Ok(());
        }
        match subscript {
            Some(subscript) => {
                let key = self.key(name, subscript)?;
                self.shell.set_element(name, key, &value.to_string())
            }
            None => self.shell.try_set_variable(name, &value.to_string()),
        }
    }

    fn comma(&mut self) -> Result<i64, String> {
        let mut value = self.assignment()?;
        while self.peek_operator() == Some(",") {
            self.position += 1;
            value = self.assignment()?;
        }
        Ok(value)
    }

    fn assignment(&mut self) -> Result<i64, String> {
        if let (Some(Token::Name(name, subscript)), Some(Token::Operator(operator))) = (
            self.tokens.get(self.position),
            self.tokens.get(self.position + 1),
        ) && ASSIGNMENTS.contains(operator)
        {
            let (name, subscript, operator) = (name.clone(), subscript.clone(), *operator);
            let operator_position = self.position + 1;
            self.position += 2;
            let right = self.assignment()?;
            let value = match operator {
                "=" => right,
                _ => {
                    let current = self.value(&name, subscript.as_deref())?;
                    self.combine(
                        &operator[..operator.len() - 1],
                        current,
                        right,
                        operator_position,
                    )?
                }
            };
            self.store(&name, subscript.as_deref(), value)?;
            return Ok(value);
        }
        let start = self.position;
        let value = self.ternary()?;
        if let Some(operator) = self.peek_operator()
            && ASSIGNMENTS.contains(&operator)
            && self.position > start
        {
            return Err(self.error("attempted assignment to non-variable"));
        }
        Ok(value)
    }

    fn combine(
        &self,
        operator: &str,
        left: i64,
        right: i64,
        position: usize,
    ) -> Result<i64, String> {
        if self.skipping > 0 {
            return Ok(0);
        }
        apply(operator, left, right).map_err(|message| self.error_at(position + 1, message))
    }

    fn skipped<T>(&mut self, skip: bool, action: impl FnOnce(&mut Self) -> T) -> T {
        if skip {
            self.skipping += 1;
        }
        let result = action(self);
        if skip {
            self.skipping -= 1;
        }
        result
    }

    fn ternary(&mut self) -> Result<i64, String> {
        let condition = self.logical_or()?;
        if self.peek_operator() != Some("?") {
            return Ok(condition);
        }
        self.position += 1;
        let when_true = self.skipped(condition == 0, |evaluator| evaluator.comma())?;
        self.expect(":", "`:' expected for conditional expression")?;
        let when_false = self.skipped(condition != 0, |evaluator| evaluator.assignment())?;
        Ok(if condition != 0 {
            when_true
        } else {
            when_false
        })
    }

    fn logical_or(&mut self) -> Result<i64, String> {
        let mut value = self.logical_and()?;
        while self.peek_operator() == Some("||") {
            self.position += 1;
            let right = self.skipped(value != 0, |evaluator| evaluator.logical_and())?;
            value = (value != 0 || right != 0) as i64;
        }
        Ok(value)
    }

    fn logical_and(&mut self) -> Result<i64, String> {
        let mut value = self.binary(3)?;
        while self.peek_operator() == Some("&&") {
            self.position += 1;
            let right = self.skipped(value == 0, |evaluator| evaluator.binary(3))?;
            value = (value != 0 && right != 0) as i64;
        }
        Ok(value)
    }

    fn binary(&mut self, min_precedence: u8) -> Result<i64, String> {
        let mut left = self.unary()?;
        self.split_increment();
        while let Some(operator) = self.peek_operator() {
            let Some(precedence) = binary_precedence(operator) else {
                break;
            };
            if precedence < min_precedence || precedence < 3 {
                break;
            }
            let operator_position = self.position;
            self.position += 1;
            let next_min_precedence = if operator == "**" {
                precedence
            } else {
                precedence + 1
            };
            let right = self.binary(next_min_precedence)?;
            left = self.combine(operator, left, right, operator_position)?;
            self.split_increment();
        }
        Ok(left)
    }

    fn step(
        &mut self,
        name: &str,
        subscript: Option<&str>,
        delta: i64,
        new: bool,
    ) -> Result<i64, String> {
        let old = self.value(name, subscript)?;
        let updated = old.wrapping_add(delta);
        self.store(name, subscript, updated)?;
        Ok(if new { updated } else { old })
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_RECURSION || crate::stack::exhausted() {
            return Err(self.error("expression recursion level exceeded"));
        }
        Ok(())
    }

    fn unary(&mut self) -> Result<i64, String> {
        self.enter()?;
        let result = self.unary_inner();
        self.depth -= 1;
        result
    }

    fn unary_inner(&mut self) -> Result<i64, String> {
        if let Some(operator @ ("++" | "--")) = self.peek_operator()
            && let Some(Token::Name(name, subscript)) = self.tokens.get(self.position + 1).cloned()
        {
            self.position += 2;
            let delta = if operator == "++" { 1 } else { -1 };
            return self.step(&name, subscript.as_deref(), delta, true);
        }
        match self.peek_operator() {
            Some("-") => {
                self.position += 1;
                Ok(self.unary()?.wrapping_neg())
            }
            Some("+") => {
                self.position += 1;
                self.unary()
            }
            Some("!") => {
                self.position += 1;
                Ok((self.unary()? == 0) as i64)
            }
            Some("~") => {
                self.position += 1;
                Ok(!self.unary()?)
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<i64, String> {
        match self.tokens.get(self.position).cloned() {
            Some(Token::Number(number)) => {
                self.position += 1;
                Ok(number)
            }
            Some(Token::Name(name, subscript)) => {
                self.position += 1;
                match self.peek_operator() {
                    Some(operator @ ("++" | "--")) => {
                        self.position += 1;
                        let delta = if operator == "++" { 1 } else { -1 };
                        self.step(&name, subscript.as_deref(), delta, false)
                    }
                    _ => self.value(&name, subscript.as_deref()),
                }
            }
            Some(Token::Operator("(")) => {
                self.position += 1;
                let value = self.comma()?;
                self.expect(")", "missing `)'")?;
                Ok(value)
            }
            _ => Err(self.error("arithmetic syntax error: operand expected")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variables::Array;

    fn calc(source: &str) -> i64 {
        eval(&mut Shell::new(), source).unwrap()
    }

    #[test]
    fn precedence() {
        assert_eq!(calc("1 + 2 * 3"), 7);
        assert_eq!(calc("(1 + 2) * 3"), 9);
        assert_eq!(calc("2 ** 3 ** 2"), 512);
        assert_eq!(calc("-3 + 5"), 2);
        assert_eq!(calc("7 % 3 == 1 && 1"), 1);
        assert_eq!(calc("0x10 + 1"), 17);
        assert_eq!(calc("2#1010 + 010"), 18);
        assert_eq!(calc("16#ff + 36#z + 64#_"), 255 + 35 + 63);
        assert_eq!(calc("3 > 2 > 1"), 0);
        assert_eq!(calc("6 & 3 | 8"), 10);
        assert_eq!(calc("1 << 65"), 2);
        assert_eq!(calc("1 ++ 2"), 3);
        assert_eq!(calc("5 -- 2"), 7);
    }

    #[test]
    fn ternary_and_comparison() {
        assert_eq!(calc("3 > 2 ? 10 : 20"), 10);
        assert_eq!(calc("!0"), 1);
    }

    #[test]
    fn comma_operator() {
        let mut shell = Shell::new();
        assert_eq!(eval(&mut shell, "a = 1, a += 2, a *= 2").unwrap(), 6);
        assert_eq!(shell.get_variable("a").as_deref(), Some("6"));
        assert_eq!(calc("1, 2, 3"), 3);
    }

    #[test]
    fn quotes_are_ignored() {
        assert_eq!(calc("\"3\" + 4"), 7);
    }

    #[test]
    fn short_circuit_skips_side_effects_and_errors() {
        let mut shell = Shell::new();
        assert_eq!(eval(&mut shell, "0 && x++").unwrap(), 0);
        assert_eq!(eval(&mut shell, "1 || (1/0)").unwrap(), 1);
        assert_eq!(eval(&mut shell, "1 ? 2 : 1/0").unwrap(), 2);
        assert_eq!(shell.get_variable("x"), None);
    }

    #[test]
    fn array_elements() {
        let mut shell = Shell::new();
        shell
            .set_array(
                "a",
                Array::indexed(vec!["1".into(), "5".into(), "9".into()]),
            )
            .unwrap();
        assert_eq!(eval(&mut shell, "a[0] + a[1]").unwrap(), 6);
        assert_eq!(eval(&mut shell, "a[1] > 2").unwrap(), 1);
        shell.set_variable("i", "2");
        assert_eq!(eval(&mut shell, "a[i]++").unwrap(), 9);
        assert_eq!(eval(&mut shell, "++a[0]").unwrap(), 2);
        assert_eq!(eval(&mut shell, "a[3] = 7").unwrap(), 7);
        assert_eq!(eval(&mut shell, "a[1] += 10").unwrap(), 15);
        assert_eq!(
            shell.arrays.get("a").unwrap().values(),
            ["2", "15", "10", "7"]
        );
        let mut map = Array::associative();
        map.set(Key::Name("k".into()), "4".into()).unwrap();
        shell.set_array("m", map).unwrap();
        assert_eq!(eval(&mut shell, "m[k] * 2").unwrap(), 8);
    }

    #[test]
    fn variables_are_evaluated_recursively() {
        let mut shell = Shell::new();
        shell.set_variable("b", "a");
        shell.set_variable("a", "5");
        shell.set_variable("e", "1+2");
        assert_eq!(eval(&mut shell, "b + 1").unwrap(), 6);
        assert_eq!(eval(&mut shell, "e * 3").unwrap(), 9);
        let error = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                let mut shell = Shell::new();
                shell.set_variable("y", "y");
                eval(&mut shell, "y").unwrap_err()
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(error.contains("recursion level exceeded"));
        shell.set_variable("z", "abc def");
        assert!(eval(&mut shell, "z").is_err());
    }

    #[test]
    fn variables_and_assignment() {
        let mut shell = Shell::new();
        shell.set_variable("n", "4");
        assert_eq!(eval(&mut shell, "n * 2").unwrap(), 8);
        assert_eq!(eval(&mut shell, "n += 1").unwrap(), 5);
        assert_eq!(shell.get_variable("n").as_deref(), Some("5"));
        assert_eq!(eval(&mut shell, "unset_thing + 1").unwrap(), 1);
    }

    #[test]
    fn increments() {
        let mut shell = Shell::new();
        shell.set_variable("i", "5");
        assert_eq!(eval(&mut shell, "i++").unwrap(), 5);
        assert_eq!(eval(&mut shell, "++i").unwrap(), 7);
        assert_eq!(eval(&mut shell, "i--").unwrap(), 7);
        assert_eq!(shell.get_variable("i").as_deref(), Some("6"));
        assert_eq!(eval(&mut shell, "2 - -1").unwrap(), 3);
    }

    #[test]
    fn errors() {
        let mut shell = Shell::new();
        assert!(
            eval(&mut shell, "1 / 0")
                .unwrap_err()
                .contains("division by 0")
        );
        assert!(eval(&mut shell, "1 +").is_err());
        assert!(eval(&mut shell, "(1").is_err());
        assert!(
            eval(&mut shell, "08")
                .unwrap_err()
                .contains("value too great")
        );
        assert!(eval(&mut shell, "2 ** -1").is_err());
        assert!(eval(&mut shell, "1 = 2").is_err());
        assert_eq!(eval(&mut shell, "10#08").unwrap(), 8);
        assert_eq!(eval(&mut shell, "").unwrap(), 0);
    }

    #[test]
    fn deep_parentheses_are_an_error_not_a_crash() {
        let source = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
        let result = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || eval(&mut Shell::new(), &source))
            .unwrap()
            .join()
            .unwrap();
        assert!(result.is_err());
    }
}
