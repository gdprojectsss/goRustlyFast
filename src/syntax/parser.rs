use std::collections::HashMap;
use std::rc::Rc;

use crate::syntax::ast::*;
use crate::syntax::lexer::{self, Spanned, Token};

pub use crate::syntax::lexer::ParseError;

type Result<T> = std::result::Result<T, ParseError>;

const RESERVED: &[&str] = &["then", "else", "elif", "fi", "do", "done", "}", "in"];

#[cfg(test)]
pub fn parse(source: &str) -> Result<List> {
    parse_with_aliases(source, &HashMap::new())
}

pub fn parse_with_aliases(source: &str, aliases: &HashMap<String, String>) -> Result<List> {
    let lexed = lexer::lex(source)?;
    let mut parser = Parser {
        tokens: lexed.tokens,
        heredocs: lexed.heredocs,
        position: 0,
        chars: source.chars().collect(),
        aliases,
    };
    let list = parser.list(&[])?;
    if !parser.at_eof() {
        return Err(parser.unexpected());
    }
    Ok(list)
}

struct Parser<'a> {
    tokens: Vec<Spanned>,
    heredocs: Vec<Word>,
    position: usize,
    chars: Vec<char>,
    aliases: &'a HashMap<String, String>,
}

pub fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

impl Parser<'_> {
    fn peek(&self) -> &Token {
        &self.tokens[self.position].token
    }

    fn peek_ahead(&self, offset: usize) -> &Token {
        let index = (self.position + offset).min(self.tokens.len() - 1);
        &self.tokens[index].token
    }

    fn next(&mut self) -> Token {
        let token = self.tokens[self.position].token.clone();
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
        token
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn is_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek(), Token::Word(word) if word.as_literal() == Some(keyword))
    }

    fn is_operator(&self, operator: &str) -> bool {
        matches!(self.peek(), Token::Operator(candidate) if *candidate == operator)
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.next();
        }
    }

    fn unexpected(&self) -> ParseError {
        let what = match self.peek() {
            Token::Eof => return ParseError::incomplete("unexpected end of input"),
            Token::Newline => "newline".to_string(),
            Token::Operator(operator) => operator.to_string(),
            Token::Word(_) => self.text(
                self.tokens[self.position].start,
                self.tokens[self.position].end,
            ),
            Token::Redirect(..) | Token::Heredoc { .. } => "redirection".to_string(),
        };
        ParseError::syntax(format!("syntax error near '{what}'"))
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<()> {
        if self.is_keyword(keyword) {
            self.next();
            Ok(())
        } else if self.at_eof() {
            Err(ParseError::incomplete(format!("expected '{keyword}'")))
        } else {
            Err(self.unexpected())
        }
    }

    fn text(&self, start: usize, end: usize) -> String {
        self.chars[start..end].iter().collect()
    }

    fn at_terminator(&self, terminators: &[&str]) -> bool {
        terminators.iter().any(|terminator| {
            if *terminator == ")" {
                self.is_operator(")")
            } else {
                self.is_keyword(terminator)
            }
        })
    }

    fn list(&mut self, terminators: &[&str]) -> Result<List> {
        let mut list = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_eof() || self.at_terminator(terminators) {
                break;
            }
            let start = self.tokens[self.position].start;
            let mut item = self.and_or()?;
            let end = self.tokens[self.position.saturating_sub(1)].end.max(start);
            item.text = self.text(start, end).trim().to_string();

            let more = match self.peek() {
                Token::Operator(";") | Token::Newline => {
                    self.next();
                    true
                }
                Token::Operator("&") => {
                    self.next();
                    item.background = true;
                    true
                }
                _ => false,
            };
            list.push(item);
            if !more {
                break;
            }
        }
        Ok(list)
    }

    fn and_or(&mut self) -> Result<AndOr> {
        let first = self.pipeline()?;
        let mut rest = Vec::new();
        loop {
            let connector = match self.peek() {
                Token::Operator("&&") => Connector::And,
                Token::Operator("||") => Connector::Or,
                _ => break,
            };
            self.next();
            self.skip_newlines();
            rest.push((connector, self.pipeline()?));
        }
        Ok(AndOr {
            first,
            rest,
            background: false,
            text: String::new(),
        })
    }

    fn pipeline(&mut self) -> Result<Pipeline> {
        let negate = self.is_keyword("!");
        if negate {
            self.next();
        }
        let mut commands = vec![self.command()?];
        while self.is_operator("|") {
            self.next();
            self.skip_newlines();
            commands.push(self.command()?);
        }
        Ok(Pipeline { negate, commands })
    }

    fn command(&mut self) -> Result<Command> {
        let kind = match self.peek().clone() {
            Token::Operator("(") => {
                self.next();
                let body = self.list(&[")"])?;
                if !self.is_operator(")") {
                    return Err(self.unexpected());
                }
                self.next();
                CommandKind::Subshell(body)
            }
            Token::Word(word) => match word.as_literal() {
                Some("if") => self.if_clause()?,
                Some(keyword @ ("while" | "until")) => {
                    let until = keyword == "until";
                    self.next();
                    let condition = self.list(&["do"])?;
                    self.expect_keyword("do")?;
                    let body = self.list(&["done"])?;
                    self.expect_keyword("done")?;
                    CommandKind::Loop {
                        condition,
                        body,
                        until,
                    }
                }
                Some("for") => self.for_clause()?,
                Some("{") => {
                    self.next();
                    let body = self.list(&["}"])?;
                    self.expect_keyword("}")?;
                    CommandKind::Group(body)
                }
                Some("function") => {
                    self.next();
                    let name = match self.next() {
                        Token::Word(word) => word
                            .as_literal()
                            .filter(|name| is_name(name))
                            .map(String::from),
                        Token::Eof => return Err(ParseError::incomplete("expected function name")),
                        _ => None,
                    };
                    let name = name.ok_or_else(|| ParseError::syntax("bad function name"))?;
                    if self.is_operator("(") {
                        self.next();
                        self.expect_operator(")")?;
                    }
                    return self.function_body(name);
                }
                Some(keyword) if RESERVED.contains(&keyword) => return Err(self.unexpected()),
                Some(name)
                    if is_name(name)
                        && matches!(self.peek_ahead(1), Token::Operator("("))
                        && matches!(self.peek_ahead(2), Token::Operator(")")) =>
                {
                    let name = name.to_string();
                    self.next();
                    self.next();
                    self.next();
                    return self.function_body(name);
                }
                _ => return self.simple(),
            },
            Token::Redirect(..) | Token::Heredoc { .. } => return self.simple(),
            _ => return Err(self.unexpected()),
        };

        let redirects = self.redirects()?;
        Ok(Command { kind, redirects })
    }

    fn expect_operator(&mut self, operator: &str) -> Result<()> {
        if self.is_operator(operator) {
            self.next();
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    fn function_body(&mut self, name: String) -> Result<Command> {
        self.skip_newlines();
        let body = self.command()?;
        if matches!(body.kind, CommandKind::Simple(_)) {
            return Err(ParseError::syntax(format!(
                "{name}: function body must be a compound command"
            )));
        }
        Ok(Command {
            kind: CommandKind::FunctionDefinition {
                name,
                body: Rc::new(body),
            },
            redirects: Vec::new(),
        })
    }

    fn if_clause(&mut self) -> Result<CommandKind> {
        self.next();
        let mut branches = Vec::new();
        let mut otherwise = None;

        let condition = self.list(&["then"])?;
        self.expect_keyword("then")?;
        let body = self.list(&["elif", "else", "fi"])?;
        branches.push((condition, body));

        loop {
            if self.is_keyword("elif") {
                self.next();
                let condition = self.list(&["then"])?;
                self.expect_keyword("then")?;
                let body = self.list(&["elif", "else", "fi"])?;
                branches.push((condition, body));
            } else if self.is_keyword("else") {
                self.next();
                otherwise = Some(self.list(&["fi"])?);
                self.expect_keyword("fi")?;
                break;
            } else {
                self.expect_keyword("fi")?;
                break;
            }
        }
        Ok(CommandKind::If {
            branches,
            otherwise,
        })
    }

    fn for_clause(&mut self) -> Result<CommandKind> {
        self.next();
        let variable = match self.next() {
            Token::Word(word) => word
                .as_literal()
                .filter(|name| is_name(name))
                .map(String::from),
            Token::Eof => return Err(ParseError::incomplete("expected loop variable")),
            _ => None,
        };
        let variable = variable.ok_or_else(|| ParseError::syntax("for: bad variable name"))?;
        self.skip_newlines();

        let mut items = None;
        if self.is_keyword("in") {
            self.next();
            let mut words = Vec::new();
            while let Token::Word(word) = self.peek().clone() {
                self.next();
                words.push(word);
            }
            items = Some(words);
        }
        match self.peek() {
            Token::Operator(";") | Token::Newline => {
                self.next();
            }
            Token::Eof => return Err(ParseError::incomplete("expected 'do'")),
            _ => {}
        }
        self.skip_newlines();
        self.expect_keyword("do")?;
        let body = self.list(&["done"])?;
        self.expect_keyword("done")?;
        Ok(CommandKind::For {
            variable,
            items,
            body,
        })
    }

    fn redirect(&mut self) -> Result<Option<Redirect>> {
        match self.peek().clone() {
            Token::Redirect(fd, kind) => {
                self.next();
                match self.next() {
                    Token::Word(target) => Ok(Some(Redirect { fd, kind, target })),
                    _ => Err(ParseError::syntax("expected filename after redirection")),
                }
            }
            Token::Heredoc { fd, index } => {
                self.next();
                let target = self.heredocs[index].clone();
                Ok(Some(Redirect {
                    fd,
                    kind: RedirectKind::Heredoc,
                    target,
                }))
            }
            _ => Ok(None),
        }
    }

    fn redirects(&mut self) -> Result<Vec<Redirect>> {
        let mut out = Vec::new();
        while let Some(redirect) = self.redirect()? {
            out.push(redirect);
        }
        Ok(out)
    }

    fn simple(&mut self) -> Result<Command> {
        let mut simple = SimpleCommand::default();
        let mut redirects = Vec::new();
        let mut expanded_aliases: Vec<String> = Vec::new();

        loop {
            if let Some(redirect) = self.redirect()? {
                redirects.push(redirect);
                continue;
            }
            let Token::Word(word) = self.peek().clone() else {
                break;
            };
            if simple.words.is_empty() {
                if let Some(assign) = split_assignment(&word) {
                    simple.assignments.push(assign);
                    self.next();
                    continue;
                }
                if let Some(name) = word.as_literal()
                    && !expanded_aliases.iter().any(|existing| existing == name)
                    && let Some(value) = self.aliases.get(name)
                {
                    expanded_aliases.push(name.to_string());
                    let value = value.clone();
                    self.splice_alias(&value)?;
                    continue;
                }
            }
            self.next();
            simple.words.push(word);
        }

        if simple.words.is_empty() && simple.assignments.is_empty() && redirects.is_empty() {
            return Err(self.unexpected());
        }
        Ok(Command {
            kind: CommandKind::Simple(simple),
            redirects,
        })
    }

    fn splice_alias(&mut self, value: &str) -> Result<()> {
        let lexed = lexer::lex(value)?;
        let (start, end) = (
            self.tokens[self.position].start,
            self.tokens[self.position].end,
        );
        let tokens: Vec<Spanned> = lexed
            .tokens
            .into_iter()
            .filter(|spanned| !matches!(spanned.token, Token::Eof | Token::Heredoc { .. }))
            .map(|spanned| Spanned {
                token: spanned.token,
                start,
                end,
            })
            .collect();
        self.tokens.splice(self.position..=self.position, tokens);
        Ok(())
    }
}

fn split_assignment(word: &Word) -> Option<(String, Word)> {
    let Some(WordPart::Literal(first)) = word.parts.first() else {
        return None;
    };
    let equals_index = first.find('=')?;
    let name = &first[..equals_index];
    if !is_name(name) {
        return None;
    }

    let rest = &first[equals_index + 1..];
    let mut parts = Vec::new();
    if let Some(after) = rest
        .strip_prefix('~')
        .filter(|after_tilde| after_tilde.is_empty() || after_tilde.starts_with('/'))
    {
        parts.push(WordPart::Tilde(String::new()));
        if !after.is_empty() {
            parts.push(WordPart::Literal(after.into()));
        }
    } else if !rest.is_empty() {
        parts.push(WordPart::Literal(rest.into()));
    }
    parts.extend(word.parts[1..].iter().cloned());
    Some((name.to_string(), Word { parts }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple(command: &Command) -> &SimpleCommand {
        match &command.kind {
            CommandKind::Simple(simple) => simple,
            other => panic!("expected simple command, got {other:?}"),
        }
    }

    fn first_command(input: &str) -> Command {
        parse(input).unwrap().remove(0).first.commands.remove(0)
    }

    fn literals(command: &Command) -> Vec<String> {
        simple(command)
            .words
            .iter()
            .map(|word| {
                word.parts
                    .iter()
                    .map(|part| match part {
                        WordPart::Literal(text) | WordPart::Quoted(text) => text.clone(),
                        _ => "<exp>".into(),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn simple_command() {
        assert_eq!(
            literals(&first_command("ls -la /tmp")),
            ["ls", "-la", "/tmp"]
        );
    }

    #[test]
    fn quoted_args() {
        assert_eq!(
            literals(&first_command("echo 'a b' \"c d\" e\\ f")),
            ["echo", "a b", "c d", "e f"]
        );
    }

    #[test]
    fn redirects() {
        let command = first_command("cmd < in > out 2>> err");
        let kinds: Vec<_> = command
            .redirects
            .iter()
            .map(|redirect| (redirect.fd, redirect.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                (0, RedirectKind::In),
                (1, RedirectKind::Out),
                (2, RedirectKind::Append)
            ]
        );
    }

    #[test]
    fn pipes_and_connectors() {
        let list = parse("a | b | c && d || e; f").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].first.commands.len(), 3);
        assert_eq!(list[0].rest[0].0, Connector::And);
        assert_eq!(list[0].rest[1].0, Connector::Or);
        assert_eq!(list[0].text, "a | b | c && d || e");
    }

    #[test]
    fn background() {
        let list = parse("sleep 1 & echo hi").unwrap();
        assert!(list[0].background);
        assert!(!list[1].background);
    }

    #[test]
    fn assignments() {
        let command = first_command("FOO=bar BAZ= env");
        let simple = simple(&command);
        assert_eq!(simple.assignments.len(), 2);
        assert_eq!(simple.assignments[0].0, "FOO");
        assert_eq!(simple.words.len(), 1);
    }

    #[test]
    fn substitution_is_deferred() {
        let command = first_command("echo $(rm -rf nothing)");
        assert!(matches!(
            simple(&command).words[1].parts[0],
            WordPart::CommandSubstitution { .. }
        ));
    }

    #[test]
    fn if_elif_else() {
        let command = first_command("if a; then b; elif c; then d; else e; fi");
        let CommandKind::If {
            branches,
            otherwise,
        } = command.kind
        else {
            panic!()
        };
        assert_eq!(branches.len(), 2);
        assert!(otherwise.is_some());
    }

    #[test]
    fn multiline_while() {
        let command = first_command("while true\ndo\n  echo x\ndone");
        assert!(matches!(
            command.kind,
            CommandKind::Loop { until: false, .. }
        ));
    }

    #[test]
    fn for_loop() {
        let command = first_command("for f in a b c; do echo $f; done");
        let CommandKind::For {
            variable, items, ..
        } = command.kind
        else {
            panic!()
        };
        assert_eq!(variable, "f");
        assert_eq!(items.unwrap().len(), 3);
    }

    #[test]
    fn function_definition() {
        let command = first_command("greet() { echo hi; }");
        assert!(
            matches!(command.kind, CommandKind::FunctionDefinition { ref name, .. } if name == "greet")
        );
        let command = first_command("function greet { echo hi; }");
        assert!(matches!(
            command.kind,
            CommandKind::FunctionDefinition { .. }
        ));
    }

    #[test]
    fn subshell_and_group_redirect() {
        let command = first_command("( echo a ) > out");
        assert!(matches!(command.kind, CommandKind::Subshell(_)));
        assert_eq!(command.redirects.len(), 1);
    }

    #[test]
    fn negation() {
        assert!(parse("! false").unwrap()[0].first.negate);
    }

    #[test]
    fn aliases_expand_in_any_command_position() {
        let aliases = HashMap::from([("ll".to_string(), "ls -l".to_string())]);
        let list = parse_with_aliases("echo ll | ll", &aliases).unwrap();
        let commands = &list[0].first.commands;
        assert_eq!(literals(&commands[0]), ["echo", "ll"]);
        assert_eq!(literals(&commands[1]), ["ls", "-l"]);
    }

    #[test]
    fn self_referencing_alias_does_not_loop() {
        let aliases = HashMap::from([("ls".to_string(), "ls --color".to_string())]);
        let list = parse_with_aliases("ls", &aliases).unwrap();
        assert_eq!(literals(&list[0].first.commands[0]), ["ls", "--color"]);
    }

    #[test]
    fn empty_input_is_empty_list() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("# just a comment").unwrap().is_empty());
    }

    #[test]
    fn syntax_errors() {
        for source in ["| ls", "&", "; ls", "ls ) x", "fi", "ls >"] {
            let err = parse(source).unwrap_err();
            assert!(!err.incomplete, "{source}: {err}");
        }
    }

    #[test]
    fn incomplete_input() {
        for source in [
            "ls |",
            "true &&",
            "if true; then",
            "while x; do",
            "for i in a",
            "{ echo",
            "f() {",
            "echo 'x",
        ] {
            let err = parse(source).unwrap_err();
            assert!(err.incomplete, "{source}: {err}");
        }
    }
}
