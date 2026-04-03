use crate::shell::Shell;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(i64),
    Name(String),
    Operator(&'static str),
}

const OPERATORS: &[&str] = &[
    "**=", "<<=", ">>=", "**", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=", "-=", "*=",
    "/=", "%=", "&=", "|=", "^=", "+", "-", "*", "/", "%", "<", ">", "&", "|", "^", "!", "~", "=",
    "(", ")", "?", ":",
];

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut rest = source.trim_start();
    while !rest.is_empty() {
        let character = rest.chars().next().unwrap_or(' ');
        if character.is_ascii_digit() {
            let end = rest
                .find(|character: char| !character.is_ascii_alphanumeric())
                .unwrap_or(rest.len());
            let text = &rest[..end];
            let number = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                Some(hex_digits) => i64::from_str_radix(hex_digits, 16),
                None => text.parse(),
            }
            .map_err(|_| format!("invalid number '{text}'"))?;
            tokens.push(Token::Number(number));
            rest = &rest[end..];
        } else if character == '_' || character.is_ascii_alphabetic() {
            let end = rest
                .find(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))
                .unwrap_or(rest.len());
            tokens.push(Token::Name(rest[..end].to_string()));
            rest = &rest[end..];
        } else if let Some(operator) = OPERATORS
            .iter()
            .find(|operator| rest.starts_with(**operator))
        {
            tokens.push(Token::Operator(operator));
            rest = &rest[operator.len()..];
        } else {
            return Err(format!("unexpected '{character}' in arithmetic"));
        }
        rest = rest.trim_start();
    }
    Ok(tokens)
}

pub fn eval(shell: &mut Shell, source: &str) -> Result<i64, String> {
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Ok(0);
    }
    let mut evaluator = Evaluator {
        tokens,
        position: 0,
        shell,
    };
    let value = evaluator.assignment()?;
    if evaluator.position < evaluator.tokens.len() {
        return Err(format!("syntax error in arithmetic: '{source}'"));
    }
    Ok(value)
}

struct Evaluator<'a> {
    tokens: Vec<Token>,
    position: usize,
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

fn apply(operator: &str, left: i64, right: i64) -> Result<i64, String> {
    Ok(match operator {
        "||" => (left != 0 || right != 0) as i64,
        "&&" => (left != 0 && right != 0) as i64,
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
        "/" | "%" if right == 0 => return Err("division by zero".into()),
        "/" => left.wrapping_div(right),
        "%" => left.wrapping_rem(right),
        "**" if right < 0 => return Err("negative exponent".into()),
        "**" => left.wrapping_pow(right as u32),
        _ => return Err(format!("unknown operator '{operator}'")),
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

    fn expect(&mut self, operator: &str) -> Result<(), String> {
        if self.peek_operator() == Some(operator) {
            self.position += 1;
            Ok(())
        } else {
            Err(format!("expected '{operator}' in arithmetic"))
        }
    }

    fn var(&mut self, name: &str) -> Result<i64, String> {
        let value = self.shell.get_variable(name).unwrap_or_default();
        let value = value.trim();
        if value.is_empty() {
            return Ok(0);
        }
        value
            .parse()
            .map_err(|_| format!("{name}: '{value}' is not a number"))
    }

    fn assignment(&mut self) -> Result<i64, String> {
        if let (Some(Token::Name(name)), Some(Token::Operator(operator))) = (
            self.tokens.get(self.position),
            self.tokens.get(self.position + 1),
        ) && operator.ends_with('=')
            && !matches!(*operator, "==" | "!=" | "<=" | ">=")
        {
            let (name, operator) = (name.clone(), *operator);
            self.position += 2;
            let right = self.assignment()?;
            let value = match operator {
                "=" => right,
                _ => apply(&operator[..operator.len() - 1], self.var(&name)?, right)?,
            };
            self.shell.set_variable(&name, &value.to_string());
            return Ok(value);
        }
        self.ternary()
    }

    fn ternary(&mut self) -> Result<i64, String> {
        let condition = self.binary(1)?;
        if self.peek_operator() != Some("?") {
            return Ok(condition);
        }
        self.position += 1;
        let when_true = self.assignment()?;
        self.expect(":")?;
        let when_false = self.assignment()?;
        Ok(if condition != 0 {
            when_true
        } else {
            when_false
        })
    }

    fn binary(&mut self, min_precedence: u8) -> Result<i64, String> {
        let mut left = self.unary()?;
        while let Some(operator) = self.peek_operator() {
            let Some(precedence) = binary_precedence(operator) else {
                break;
            };
            if precedence < min_precedence {
                break;
            }
            self.position += 1;
            let next_min_precedence = if operator == "**" {
                precedence
            } else {
                precedence + 1
            };
            let right = self.binary(next_min_precedence)?;
            left = apply(operator, left, right)?;
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<i64, String> {
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
            Some(Token::Name(name)) => {
                self.position += 1;
                self.var(&name)
            }
            Some(Token::Operator("(")) => {
                self.position += 1;
                let value = self.assignment()?;
                self.expect(")")?;
                Ok(value)
            }
            Some(Token::Operator(operator)) => {
                Err(format!("unexpected '{operator}' in arithmetic"))
            }
            None => Err("unexpected end of arithmetic expression".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn ternary_and_comparison() {
        assert_eq!(calc("3 > 2 ? 10 : 20"), 10);
        assert_eq!(calc("!0"), 1);
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
    fn errors() {
        let mut shell = Shell::new();
        assert!(eval(&mut shell, "1 / 0").is_err());
        assert!(eval(&mut shell, "1 +").is_err());
        assert!(eval(&mut shell, "(1").is_err());
    }
}
