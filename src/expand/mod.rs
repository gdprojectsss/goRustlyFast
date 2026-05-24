mod arith;
pub mod glob;

use crate::shell::Shell;
use crate::syntax::ast::{ParameterOperation, ParameterOperationKind, Word, WordPart};
use crate::sys;

pub fn words(shell: &mut Shell, words: &[Word]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for word in words {
        let mut fields = Fields::new(true);
        expand_into(shell, word, &mut fields)?;
        out.extend(fields.finish());
    }
    Ok(out)
}

pub fn single(shell: &mut Shell, word: &Word) -> Result<String, String> {
    let mut fields = Fields::new(false);
    expand_into(shell, word, &mut fields)?;
    Ok(fields.current)
}

fn expand_into(shell: &mut Shell, word: &Word, fields: &mut Fields) -> Result<(), String> {
    for part in &word.parts {
        match part {
            WordPart::Literal(text) => fields.literal(text),
            WordPart::Quoted(text) => fields.quoted(text),
            WordPart::Tilde(user) => fields.quoted(&tilde(shell, user)),
            WordPart::Parameter {
                name,
                quoted: true,
                operator: None,
            } if name == "@" => {
                for (index, argument) in shell.positional.clone().iter().enumerate() {
                    if index > 0 {
                        fields.break_field();
                    }
                    fields.quoted(argument);
                }
            }
            WordPart::Parameter {
                name,
                operator,
                quoted,
            } => {
                let value = parameter(shell, name, operator.as_deref())?;
                if *quoted {
                    fields.quoted(&value);
                } else {
                    fields.split(&value);
                }
            }
            WordPart::Length(name) => {
                let length = shell
                    .get_variable(name)
                    .map_or(0, |value| value.chars().count());
                fields.quoted(&length.to_string());
            }
            WordPart::CommandSubstitution { source, quoted } => {
                let out = shell.capture(source);
                if *quoted {
                    fields.quoted(&out);
                } else {
                    fields.split(&out);
                }
            }
            WordPart::Arithmetic { expression, .. } => {
                let text = single(shell, expression)?;
                let value = arith::eval(shell, &text)?;
                fields.quoted(&value.to_string());
            }
        }
    }
    Ok(())
}

fn parameter(
    shell: &mut Shell,
    name: &str,
    operator: Option<&ParameterOperation>,
) -> Result<String, String> {
    let value = shell.get_variable(name);
    let Some(operator) = operator else {
        return Ok(value.unwrap_or_default());
    };
    let unset = match &value {
        None => true,
        Some(value) => operator.colon && value.is_empty(),
    };
    match operator.kind {
        ParameterOperationKind::Default if unset => single(shell, &operator.word),
        ParameterOperationKind::Assign if unset => {
            let value = single(shell, &operator.word)?;
            shell.set_variable(name, &value);
            Ok(value)
        }
        ParameterOperationKind::Alternative if unset => Ok(String::new()),
        ParameterOperationKind::Alternative => single(shell, &operator.word),
        ParameterOperationKind::Error if unset => {
            let message = single(shell, &operator.word)?;
            let message = if message.is_empty() {
                "parameter null or not set".into()
            } else {
                message
            };
            Err(format!("{name}: {message}"))
        }
        _ => Ok(value.unwrap_or_default()),
    }
}

fn tilde(shell: &Shell, user: &str) -> String {
    let found = match user {
        "" => shell.get_variable("HOME"),
        "+" => shell.get_variable("PWD"),
        "-" => shell.get_variable("OLDPWD"),
        _ => sys::user_home(user),
    };
    found.unwrap_or_else(|| format!("~{user}"))
}

struct Fields {
    split: bool,
    done: Vec<(String, String, bool)>,
    current: String,
    pattern: String,
    glob: bool,
    started: bool,
}

fn is_field_separator(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n')
}

impl Fields {
    fn new(split: bool) -> Self {
        Fields {
            split,
            done: Vec::new(),
            current: String::new(),
            pattern: String::new(),
            glob: false,
            started: false,
        }
    }

    fn literal(&mut self, text: &str) {
        self.started = true;
        self.current.push_str(text);
        self.pattern.push_str(text);
        if self.split && glob::has_magic(text) {
            self.glob = true;
        }
    }

    fn quoted(&mut self, text: &str) {
        self.started = true;
        self.current.push_str(text);
        for character in text.chars() {
            if matches!(character, '*' | '?' | '[' | ']' | '\\') {
                self.pattern.push('\\');
            }
            self.pattern.push(character);
        }
    }

    fn split(&mut self, text: &str) {
        if !self.split {
            self.quoted(text);
            return;
        }
        if text.starts_with(is_field_separator) {
            self.break_field();
        }
        let mut pieces = text
            .split(is_field_separator)
            .filter(|argument| !argument.is_empty())
            .peekable();
        while let Some(piece) = pieces.next() {
            self.literal(piece);
            if pieces.peek().is_some() {
                self.break_field();
            }
        }
        if text.ends_with(is_field_separator) {
            self.break_field();
        }
    }

    fn break_field(&mut self) {
        if self.started {
            let current = std::mem::take(&mut self.current);
            let pattern = std::mem::take(&mut self.pattern);
            self.done.push((current, pattern, self.glob));
            self.glob = false;
            self.started = false;
        }
    }

    fn finish(mut self) -> Vec<String> {
        self.break_field();
        let mut out = Vec::new();
        for (text, pattern, glob) in self.done {
            match glob.then(|| glob::expand(&pattern)).flatten() {
                Some(matches) => out.extend(matches),
                None if glob => out.push(glob::unescape(&pattern)),
                None => out.push(text),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::lexer::{self, Token};

    fn expand(shell: &mut Shell, source: &str) -> Vec<String> {
        let word_list: Vec<Word> = lexer::lex(source)
            .unwrap()
            .tokens
            .into_iter()
            .filter_map(|t| match t.token {
                Token::Word(word) => Some(word),
                _ => None,
            })
            .collect();
        words(shell, &word_list).unwrap()
    }

    #[test]
    fn unquoted_expansion_splits() {
        let mut shell = Shell::new();
        shell.set_variable("x", "a  b c");
        assert_eq!(expand(&mut shell, "$x"), ["a", "b", "c"]);
        assert_eq!(expand(&mut shell, "\"$x\""), ["a  b c"]);
        assert_eq!(expand(&mut shell, "pre$x"), ["prea", "b", "c"]);
    }

    #[test]
    fn empty_unquoted_expansion_vanishes() {
        let mut shell = Shell::new();
        assert_eq!(expand(&mut shell, "$nothing x"), ["x"]);
        assert_eq!(expand(&mut shell, "\"$nothing\""), [""]);
    }

    #[test]
    fn positional_at() {
        let mut shell = Shell::new();
        shell.positional = vec!["a b".into(), "c".into()];
        assert_eq!(expand(&mut shell, "\"$@\""), ["a b", "c"]);
        assert_eq!(expand(&mut shell, "$@"), ["a", "b", "c"]);
        assert_eq!(expand(&mut shell, "\"$*\""), ["a b c"]);
        assert_eq!(expand(&mut shell, "$#"), ["2"]);
    }

    #[test]
    fn parameter_operators() {
        let mut shell = Shell::new();
        shell.set_variable("empty", "");
        assert_eq!(expand(&mut shell, "${unset:-dflt}"), ["dflt"]);
        assert_eq!(expand(&mut shell, "${empty:-dflt}"), ["dflt"]);
        assert_eq!(expand(&mut shell, "${empty-dflt}"), [] as [&str; 0]);
        assert_eq!(expand(&mut shell, "${new:=set}"), ["set"]);
        assert_eq!(shell.get_variable("new").as_deref(), Some("set"));
        assert_eq!(expand(&mut shell, "${new:+alt}"), ["alt"]);
        assert_eq!(expand(&mut shell, "${#new}"), ["3"]);
    }

    #[test]
    fn arithmetic() {
        let mut shell = Shell::new();
        shell.set_variable("i", "41");
        assert_eq!(expand(&mut shell, "$((i + 1))"), ["42"]);
        assert_eq!(expand(&mut shell, "$(( $i * 2 ))"), ["82"]);
    }

    #[test]
    fn quoted_glob_stays_literal() {
        let mut shell = Shell::new();
        assert_eq!(expand(&mut shell, "'src/*.rs'"), ["src/*.rs"]);
        assert!(expand(&mut shell, "src/*.rs").contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn unmatched_glob_is_kept() {
        let mut shell = Shell::new();
        assert_eq!(expand(&mut shell, "nope_*.zzz"), ["nope_*.zzz"]);
    }

    #[test]
    fn tilde() {
        let mut shell = Shell::new();
        shell.variables.insert("HOME".into(), "/home/me".into());
        assert_eq!(expand(&mut shell, "~/x '~'"), ["/home/me/x", "~"]);
    }
}
