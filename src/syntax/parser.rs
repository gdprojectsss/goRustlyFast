use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

use crate::syntax::ast::*;
use crate::syntax::lexer::{self, Lexer, LineSource, Spanned, Token};

pub use crate::syntax::lexer::ParseError;

type Result<T> = std::result::Result<T, ParseError>;

const UNARY_CONDITIONS: &[&str] = crate::builtins::test::UNARY;
const BINARY_CONDITIONS: &[&str] = &[
    "=", "==", "!=", "=~", "-eq", "-ne", "-lt", "-le", "-gt", "-ge", "-nt", "-ot", "-ef",
];

const DECLARATION_BUILTINS: &[&str] = &["declare", "typeset", "local", "export", "readonly"];

const RESERVED: &[&str] = &[
    "then", "else", "elif", "fi", "do", "done", "}", "in", "esac", "]]",
];

const COMPOUND_STARTS: &[&str] = &["{", "if", "while", "until", "for", "case", "select", "[["];

pub fn parse(source: &str) -> Result<List> {
    parse_with_aliases(source, &HashMap::new())
}

pub fn parse_with_aliases(source: &str, aliases: &HashMap<String, String>) -> Result<List> {
    parse_from_line(source, aliases, 1)
}

pub fn parse_from_line(
    source: &str,
    aliases: &HashMap<String, String>,
    first_line: usize,
) -> Result<List> {
    parse_lines(source, aliases, first_line, false)
}

pub fn parse_with_extglob(source: &str) -> Result<List> {
    parse_lines(source, &HashMap::new(), 1, true)
}

fn parse_lines(
    source: &str,
    aliases: &HashMap<String, String>,
    first_line: usize,
    always_extglob: bool,
) -> Result<List> {
    let mut parser = Parser::new(Lexer::new(source).starting_at_line(first_line));
    parser.aliases = Cow::Borrowed(aliases);
    parser.always_extglob = always_extglob;
    let mut list = Vec::new();
    while let Some(unit) = parser.next_unit()? {
        list.extend(unit);
    }
    Ok(list)
}

pub(crate) fn substitution_end(lexer: &mut Lexer<'_>) -> Result<usize> {
    let taken = std::mem::replace(lexer, Lexer::new(""));
    let mut parser = Parser::new(taken);
    let condition = parser.lexer.condition;
    parser.lexer.condition = false;
    let outcome = parser.substitution_body();
    let mut restored = parser.lexer;
    restored.condition = condition;
    *lexer = restored;
    outcome
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    tokens: Vec<Spanned>,
    position: usize,
    aliases: Cow<'a, HashMap<String, String>>,
    failed: Option<ParseError>,
    always_extglob: bool,
}

pub fn is_function_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|character: char| character.is_ascii_digit())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-.:+@".contains(character))
}

pub fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn stderr_to_stdout() -> Redirect {
    Redirect {
        fd: 2,
        kind: RedirectKind::DuplicateOut,
        target: Word {
            parts: vec![WordPart::Literal("1".into())],
        },
        variable: None,
    }
}

impl<'a> Parser<'a> {
    fn new(lexer: Lexer<'a>) -> Self {
        Parser {
            lexer,
            tokens: Vec::new(),
            position: 0,
            aliases: Cow::Owned(HashMap::new()),
            failed: None,
            always_extglob: false,
        }
    }

    pub fn for_text(text: &str, first_line: usize) -> Parser<'static> {
        let mut lexer = Lexer::new(text).starting_at_line(first_line);
        lexer.final_input = true;
        Parser::new(lexer)
    }

    pub fn for_lines(source: LineSource<'a>) -> Self {
        let mut lexer = Lexer::from_lines(source);
        lexer.final_input = true;
        Parser::new(lexer)
    }

    pub fn set_aliases(&mut self, aliases: &HashMap<String, String>) {
        if *self.aliases != *aliases {
            self.aliases = Cow::Owned(aliases.clone());
        }
    }

    pub fn take_warnings(&mut self) -> Vec<ParseError> {
        std::mem::take(&mut self.lexer.warnings)
    }

    pub fn next_unit(&mut self) -> Result<Option<List>> {
        let result = self.unit();
        let result = match self.failed.take() {
            Some(error) => Err(error),
            None => result,
        };
        result.map_err(|error| self.finish_error(error))
    }

    fn finish_error(&self, error: ParseError) -> ParseError {
        if error.incomplete && self.lexer.final_input {
            let line = self.lexer.line_at(self.lexer.position().saturating_sub(1));
            return ParseError::syntax(format!("unexpected end of file: {}", error.message))
                .at_line(line);
        }
        self.located(error)
    }

    fn unit(&mut self) -> Result<Option<List>> {
        self.tokens.drain(..self.position);
        self.position = 0;
        if self.tokens.is_empty() {
            self.lexer.release_consumed();
        }
        self.lexer.condition = false;
        self.lexer.extglob = self.always_extglob || crate::expand::glob::extglob();
        loop {
            match self.peek() {
                Token::Newline => {
                    self.next();
                }
                Token::Eof => return Ok(None),
                _ => break,
            }
        }
        let mut list = Vec::new();
        loop {
            let mut item = self.list_item()?;
            let mut done = false;
            match self.peek() {
                Token::Operator(";") => {
                    self.next();
                }
                Token::Operator("&") => {
                    self.next();
                    item.background = true;
                }
                Token::Newline => {
                    self.next();
                    done = true;
                }
                Token::Eof => done = true,
                _ => return Err(self.unexpected()),
            }
            list.push(item);
            if done {
                break;
            }
            match self.peek() {
                Token::Newline => {
                    self.next();
                    break;
                }
                Token::Eof => break,
                _ => {}
            }
        }
        Ok(Some(list))
    }

    fn substitution_body(&mut self) -> Result<usize> {
        let result = self.list(&[")"]).and_then(|_| {
            if self.is_operator(")") {
                Ok(self.tokens[self.position].start)
            } else if self.at_eof() {
                Err(ParseError::incomplete("unclosed $("))
            } else {
                Err(self.unexpected())
            }
        });
        let result = match self.failed.take() {
            Some(error) => Err(error),
            None => result,
        };
        result.map_err(|error| self.located(error))
    }

    fn fill(&mut self, index: usize) {
        while self.tokens.len() <= index {
            if self
                .tokens
                .last()
                .is_some_and(|spanned| spanned.token == Token::Eof)
            {
                return;
            }
            let spanned = match self.lexer.next_token() {
                Ok(spanned) => spanned,
                Err(error) => {
                    let end = self.lexer.position();
                    let error = error.at_line(self.lexer.line_at(end));
                    self.failed.get_or_insert(error);
                    Spanned {
                        token: Token::Eof,
                        start: end,
                        end,
                    }
                }
            };
            self.tokens.push(spanned);
        }
    }

    fn current(&mut self) -> &Spanned {
        self.fill(self.position);
        let index = self.position.min(self.tokens.len() - 1);
        &self.tokens[index]
    }

    fn peek(&mut self) -> &Token {
        &self.current().token
    }

    fn peek_ahead(&mut self, offset: usize) -> &Token {
        self.fill(self.position + offset);
        let index = (self.position + offset).min(self.tokens.len() - 1);
        &self.tokens[index].token
    }

    fn next(&mut self) -> Token {
        let token = self.peek().clone();
        if token != Token::Eof {
            self.position += 1;
        }
        token
    }

    fn start(&mut self) -> usize {
        self.current().start
    }

    fn required_list(&mut self, terminators: &[&str]) -> Result<List> {
        let list = self.list(terminators)?;
        if list.is_empty() {
            return Err(self.located_unexpected());
        }
        Ok(list)
    }

    fn at_eof(&mut self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn is_keyword(&mut self, keyword: &str) -> bool {
        matches!(self.peek(), Token::Word(word) if word.as_literal() == Some(keyword))
    }

    fn is_operator(&mut self, operator: &str) -> bool {
        matches!(self.peek(), Token::Operator(candidate) if *candidate == operator)
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.next();
        }
    }

    fn unexpected(&mut self) -> ParseError {
        let spanned = self.current().clone();
        let what = match &spanned.token {
            Token::Eof => return ParseError::incomplete("unexpected end of input"),
            Token::Newline => "newline".to_string(),
            Token::Operator(operator) => operator.to_string(),
            Token::Word(_) => self.text(spanned.start, spanned.end),
            Token::Redirect(..) | Token::NamedRedirect(..) | Token::Heredoc { .. } => {
                "redirection".to_string()
            }
            Token::Arithmetic(_) => "((".to_string(),
        };
        ParseError::syntax(format!("syntax error near '{what}'"))
    }

    fn located_unexpected(&mut self) -> ParseError {
        let error = self.unexpected();
        self.located(error)
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

    fn located(&self, error: ParseError) -> ParseError {
        if error.incomplete || error.line.is_some() {
            return error;
        }
        let index = self
            .tokens
            .get(self.position.min(self.tokens.len().saturating_sub(1)))
            .map_or(self.lexer.position(), |spanned| spanned.start);
        error.at_line(self.lexer.line_at(index))
    }

    fn text(&self, start: usize, end: usize) -> String {
        self.lexer.text(start, end)
    }

    fn at_terminator(&mut self, terminators: &[&str]) -> bool {
        terminators.iter().any(|terminator| {
            if matches!(*terminator, ")" | ";;" | ";&" | ";;&") {
                self.is_operator(terminator)
            } else {
                self.is_keyword(terminator)
            }
        })
    }

    fn list_item(&mut self) -> Result<AndOr> {
        let start = self.start();
        let mut item = self.and_or()?;
        let end = if self.position == 0 {
            start
        } else {
            self.tokens[self.position - 1].end.max(start)
        };
        item.text = self.text(start, end).trim().to_string();
        item.line = self.lexer.line_at(start);
        Ok(item)
    }

    fn list(&mut self, terminators: &[&str]) -> Result<List> {
        let mut list = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_eof() || self.at_terminator(terminators) {
                break;
            }
            let mut item = self.list_item()?;
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
            line: 0,
        })
    }

    fn pipeline(&mut self) -> Result<Pipeline> {
        let timed = self.is_keyword("time");
        if timed {
            self.next();
        }
        let mut negate = false;
        while self.is_keyword("!") {
            self.next();
            negate = !negate;
        }
        let mut commands = vec![self.command()?];
        loop {
            if self.is_operator("|&") {
                if let Some(last) = commands.last_mut() {
                    last.redirects.push(stderr_to_stdout());
                }
            } else if !self.is_operator("|") {
                break;
            }
            self.next();
            self.skip_newlines();
            commands.push(self.command()?);
        }
        Ok(Pipeline {
            negate,
            timed,
            commands,
        })
    }

    fn command(&mut self) -> Result<Command> {
        self.lexer.depth += 1;
        let result = if self.lexer.depth > lexer::MAX_NESTING || crate::stack::exhausted() {
            Err(ParseError::too_deep())
        } else {
            self.command_inner()
        };
        self.lexer.depth -= 1;
        result
    }

    fn command_inner(&mut self) -> Result<Command> {
        let kind = match self.peek().clone() {
            Token::Arithmetic(source) => {
                self.next();
                CommandKind::Arithmetic(lexer::parse_double_quoted_text(&source)?)
            }
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
                    let condition = self.required_list(&["do"])?;
                    self.expect_keyword("do")?;
                    let body = self.required_list(&["done"])?;
                    self.expect_keyword("done")?;
                    CommandKind::Loop {
                        condition,
                        body,
                        until,
                    }
                }
                Some("for") => self.for_clause()?,
                Some("select") if self.select_is_loop() => self.select_clause()?,
                Some("case") => self.case_clause()?,
                Some("coproc") => return self.coproc(),
                Some("[[") => {
                    self.next();
                    self.lexer.condition = true;
                    let condition = self.condition_or();
                    let condition = condition.and_then(|condition| {
                        self.expect_keyword("]]")?;
                        Ok(condition)
                    });
                    self.lexer.condition = false;
                    CommandKind::Conditional(condition?)
                }
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
                            .filter(|name| is_function_name(name))
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
                    if is_function_name(name)
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
            Token::Redirect(..) | Token::NamedRedirect(..) | Token::Heredoc { .. } => {
                return self.simple();
            }
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

    fn coproc(&mut self) -> Result<Command> {
        self.next();
        let named = match self.peek() {
            Token::Word(word) => word
                .as_literal()
                .filter(|name| is_name(name))
                .map(String::from),
            _ => None,
        };
        let compound_follows = match self.peek_ahead(1) {
            Token::Word(word) => word
                .as_literal()
                .is_some_and(|keyword| COMPOUND_STARTS.contains(&keyword)),
            Token::Operator("(") | Token::Arithmetic(_) => true,
            _ => false,
        };
        let name = match named {
            Some(name) if compound_follows => {
                self.next();
                name
            }
            _ => "COPROC".to_string(),
        };
        let body = self.command()?;
        Ok(Command {
            kind: CommandKind::Coproc {
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

        let condition = self.required_list(&["then"])?;
        self.expect_keyword("then")?;
        let body = self.required_list(&["elif", "else", "fi"])?;
        branches.push((condition, body));

        loop {
            if self.is_keyword("elif") {
                self.next();
                let condition = self.required_list(&["then"])?;
                self.expect_keyword("then")?;
                let body = self.required_list(&["elif", "else", "fi"])?;
                branches.push((condition, body));
            } else if self.is_keyword("else") {
                self.next();
                otherwise = Some(self.required_list(&["fi"])?);
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

    fn condition_word(&mut self) -> Result<Word> {
        match self.next() {
            Token::Word(word) => Ok(word),
            Token::Eof => Err(ParseError::incomplete("expected ']]'")),
            _ => Err(ParseError::syntax("unexpected token in [[ ]]")),
        }
    }

    fn regex_operand(&mut self) -> Result<Word> {
        let start = self.start();
        let mut end = start;
        let mut depth = 0usize;
        loop {
            match self.peek() {
                Token::Eof | Token::Newline => {
                    return Err(ParseError::incomplete("expected ']]'"));
                }
                Token::Word(word) if depth == 0 && word.as_literal() == Some("]]") => break,
                Token::Operator("&&" | "||") if depth == 0 => break,
                Token::Operator("(") => depth += 1,
                Token::Operator(")") if depth == 0 => break,
                Token::Operator(")") => depth -= 1,
                _ => {}
            }
            end = self.current().end;
            self.next();
        }
        if end == start {
            return Err(ParseError::syntax("expected regular expression after '=~'"));
        }
        lexer::parse_regex_word(&self.text(start, end))
    }

    fn condition_or(&mut self) -> Result<Condition> {
        let mut condition = self.condition_and()?;
        while self.is_operator("||") {
            self.next();
            self.skip_newlines();
            let right = self.condition_and()?;
            condition = Condition::Or(Box::new(condition), Box::new(right));
        }
        Ok(condition)
    }

    fn condition_and(&mut self) -> Result<Condition> {
        let mut condition = self.condition_not()?;
        while self.is_operator("&&") {
            self.next();
            self.skip_newlines();
            let right = self.condition_not()?;
            condition = Condition::And(Box::new(condition), Box::new(right));
        }
        Ok(condition)
    }

    fn condition_not(&mut self) -> Result<Condition> {
        self.lexer.depth += 1;
        let result = if self.lexer.depth > lexer::MAX_NESTING || crate::stack::exhausted() {
            Err(ParseError::too_deep())
        } else {
            self.condition_primary()
        };
        self.lexer.depth -= 1;
        result
    }

    fn condition_primary(&mut self) -> Result<Condition> {
        self.skip_newlines();
        if self.is_keyword("!") {
            self.next();
            return Ok(Condition::Not(Box::new(self.condition_not()?)));
        }
        if self.is_operator("(") {
            self.next();
            let inner = self.condition_or()?;
            self.skip_newlines();
            self.expect_operator(")")?;
            return Ok(inner);
        }
        if self.at_eof() {
            return Err(ParseError::incomplete("expected ']]'"));
        }

        let first = self.condition_word()?;
        let binary = match self.peek() {
            Token::Word(word) => word
                .as_literal()
                .filter(|operator| BINARY_CONDITIONS.contains(operator))
                .map(String::from),
            Token::Redirect(_, RedirectKind::In) => Some("<".to_string()),
            Token::Redirect(_, RedirectKind::Out) => Some(">".to_string()),
            _ => None,
        };
        if let Some(operator) = binary {
            self.next();
            let right = if operator == "=~" {
                self.regex_operand()?
            } else {
                self.condition_word()?
            };
            return Ok(Condition::Binary {
                operator,
                left: first,
                right,
            });
        }
        if let Some(operator) = first
            .as_literal()
            .filter(|operator| UNARY_CONDITIONS.contains(operator))
            && !self.is_keyword("]]")
        {
            let operator = operator.to_string();
            let operand = self.condition_word()?;
            return Ok(Condition::Unary { operator, operand });
        }
        Ok(Condition::Single(first))
    }

    fn case_clause(&mut self) -> Result<CommandKind> {
        self.next();
        let subject = match self.next() {
            Token::Word(word) => word,
            Token::Eof => return Err(ParseError::incomplete("expected word after 'case'")),
            _ => return Err(ParseError::syntax("expected word after 'case'")),
        };
        self.skip_newlines();
        self.expect_keyword("in")?;

        let mut arms = Vec::new();
        loop {
            self.skip_newlines();
            if self.is_keyword("esac") {
                self.next();
                break;
            }
            if self.at_eof() {
                return Err(ParseError::incomplete("expected 'esac'"));
            }
            if self.is_operator("(") {
                self.next();
            }
            let mut patterns = Vec::new();
            loop {
                match self.next() {
                    Token::Word(word) => patterns.push(word),
                    Token::Eof => return Err(ParseError::incomplete("expected case pattern")),
                    _ => return Err(ParseError::syntax("expected case pattern")),
                }
                if self.is_operator("|") {
                    self.next();
                } else {
                    break;
                }
            }
            if !self.is_operator(")") {
                return Err(self.unexpected());
            }
            self.next();
            let body = self.list(&[";;", ";&", ";;&", "esac"])?;
            let end = if self.is_operator(";&") {
                CaseEnd::FallThrough
            } else if self.is_operator(";;&") {
                CaseEnd::TestNext
            } else {
                CaseEnd::Stop
            };
            if self.is_operator(";;") || self.is_operator(";&") || self.is_operator(";;&") {
                self.next();
            } else if !self.is_keyword("esac") {
                return Err(self.unexpected());
            }
            arms.push(CaseArm {
                patterns,
                body,
                end,
            });
        }
        Ok(CommandKind::Case { subject, arms })
    }

    fn loop_body(&mut self) -> Result<List> {
        if self.is_keyword("{") {
            self.next();
            let body = self.list(&["}"])?;
            self.expect_keyword("}")?;
            return Ok(body);
        }
        self.expect_keyword("do")?;
        let body = self.required_list(&["done"])?;
        self.expect_keyword("done")?;
        Ok(body)
    }

    fn for_clause(&mut self) -> Result<CommandKind> {
        self.next();
        if let Token::Arithmetic(source) = self.peek().clone() {
            self.next();
            let parts: Vec<&str> = source.split(';').collect();
            let [init, condition, step] = parts.as_slice() else {
                return Err(ParseError::syntax(
                    "for (( )) needs three expressions separated by ';'",
                ));
            };
            let init = lexer::parse_double_quoted_text(init)?;
            let condition = lexer::parse_double_quoted_text(condition)?;
            let step = lexer::parse_double_quoted_text(step)?;
            if self.is_operator(";") {
                self.next();
            }
            self.skip_newlines();
            let body = self.loop_body()?;
            return Ok(CommandKind::ArithmeticFor {
                init,
                condition,
                step,
                body,
            });
        }
        let (variable, items) = self.loop_header("for")?;
        let body = self.loop_body()?;
        Ok(CommandKind::For {
            variable,
            items,
            body,
        })
    }

    fn select_is_loop(&mut self) -> bool {
        let named = matches!(
            self.peek_ahead(1),
            Token::Word(word) if word.as_literal().is_some_and(is_name)
        );
        if !named {
            return false;
        }
        let mut offset = 2;
        if matches!(self.peek_ahead(offset), Token::Operator(";")) {
            offset += 1;
        }
        while matches!(self.peek_ahead(offset), Token::Newline) {
            offset += 1;
        }
        matches!(
            self.peek_ahead(offset),
            Token::Word(word) if matches!(word.as_literal(), Some("in" | "do"))
        )
    }

    fn select_clause(&mut self) -> Result<CommandKind> {
        self.next();
        let (variable, items) = self.loop_header("select")?;
        let body = self.loop_body()?;
        Ok(CommandKind::Select {
            variable,
            items,
            body,
        })
    }

    fn loop_header(&mut self, keyword: &str) -> Result<(String, Option<Vec<Word>>)> {
        let variable = match self.next() {
            Token::Word(word) => word
                .as_literal()
                .filter(|name| is_name(name))
                .map(String::from),
            Token::Eof => return Err(ParseError::incomplete("expected loop variable")),
            _ => None,
        };
        let variable =
            variable.ok_or_else(|| ParseError::syntax(format!("{keyword}: bad variable name")))?;
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
        Ok((variable, items))
    }

    fn finish_assignment(&mut self, target: AssignmentTarget) -> Result<Assignment> {
        let value = if target.value.is_empty() && target.index.is_none() && self.is_operator("(") {
            AssignedValue::Array(self.array_elements()?)
        } else {
            AssignedValue::Scalar(Word {
                parts: target.value,
            })
        };
        Ok(Assignment {
            name: target.name,
            index: target.index,
            append: target.append,
            value,
        })
    }

    fn array_elements(&mut self) -> Result<Vec<Word>> {
        self.next();
        let mut elements = Vec::new();
        loop {
            match self.next() {
                Token::Word(word) => elements.push(word),
                Token::Newline => {}
                Token::Operator(")") => return Ok(elements),
                Token::Eof => return Err(ParseError::incomplete("unclosed array")),
                _ => return Err(ParseError::syntax("unexpected token in array")),
            }
        }
    }

    fn redirect_target(&mut self) -> Result<Word> {
        match self.next() {
            Token::Word(target) => Ok(target),
            _ => Err(ParseError::syntax("expected filename after redirection")),
        }
    }

    fn redirect(&mut self) -> Result<Option<Redirect>> {
        match self.peek().clone() {
            Token::Redirect(fd, kind) => {
                self.next();
                let target = self.redirect_target()?;
                Ok(Some(Redirect {
                    fd,
                    kind,
                    target,
                    variable: None,
                }))
            }
            Token::NamedRedirect(name, kind) => {
                self.next();
                let target = self.redirect_target()?;
                Ok(Some(Redirect {
                    fd: -1,
                    kind,
                    target,
                    variable: Some(name),
                }))
            }
            Token::Heredoc { fd, index } => {
                self.next();
                self.complete_heredocs();
                Ok(Some(Redirect {
                    fd,
                    kind: RedirectKind::Heredoc,
                    target: self.lexer.heredoc_body(index),
                    variable: None,
                }))
            }
            _ => Ok(None),
        }
    }

    fn complete_heredocs(&mut self) {
        while self.lexer.has_pending_heredocs() {
            let length = self.tokens.len();
            self.fill(length);
            if self.tokens.len() == length {
                break;
            }
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
            let declaring = simple
                .words
                .first()
                .and_then(Word::as_literal)
                .is_some_and(|command| DECLARATION_BUILTINS.contains(&command));
            if declaring && let Some(target) = assignment_target(&word) {
                self.next();
                let assignment = self.finish_assignment(target)?;
                simple.declarations.push(assignment);
                continue;
            }
            if simple.words.is_empty() {
                if let Some(target) = assignment_target(&word) {
                    self.next();
                    let assignment = self.finish_assignment(target)?;
                    simple.assignments.push(assignment);
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

struct AssignmentTarget {
    name: String,
    index: Option<Word>,
    append: bool,
    value: Vec<WordPart>,
}

fn assignment_target(word: &Word) -> Option<AssignmentTarget> {
    let Some(WordPart::Literal(first)) = word.parts.first() else {
        return None;
    };
    let name_end =
        first.find(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))?;
    let name = &first[..name_end];
    if !is_name(name) {
        return None;
    }

    let mut rest_parts: Vec<WordPart> = Vec::new();
    let after_name = &first[name_end..];
    let (index, operator_and_value): (Option<Word>, String) =
        if let Some(inside) = after_name.strip_prefix('[') {
            let mut index_parts = Vec::new();
            let mut pending = inside.to_string();
            let mut remaining = word.parts[1..].iter();
            loop {
                if let Some(close) = pending.find(']') {
                    if close > 0 {
                        index_parts.push(WordPart::Literal(pending[..close].to_string()));
                    }
                    let after = pending[close + 1..].to_string();
                    rest_parts.extend(remaining.cloned());
                    break (Some(Word { parts: index_parts }), after);
                }
                if !pending.is_empty() {
                    index_parts.push(WordPart::Literal(std::mem::take(&mut pending)));
                }
                match remaining.next()? {
                    WordPart::Literal(text) => pending = text.clone(),
                    other => index_parts.push(other.clone()),
                }
            }
        } else {
            rest_parts.extend(word.parts[1..].iter().cloned());
            (None, after_name.to_string())
        };

    let (append, value_text) = if let Some(value) = operator_and_value.strip_prefix("+=") {
        (true, value)
    } else {
        (false, operator_and_value.strip_prefix('=')?)
    };

    let mut value = Vec::new();
    if let Some(after) = value_text
        .strip_prefix('~')
        .filter(|after_tilde| after_tilde.is_empty() || after_tilde.starts_with('/'))
    {
        value.push(WordPart::Tilde(String::new()));
        if !after.is_empty() {
            value.push(WordPart::Literal(after.into()));
        }
    } else if !value_text.is_empty() {
        value.push(WordPart::Literal(value_text.into()));
    }
    value.extend(rest_parts);
    Some(AssignmentTarget {
        name: name.to_string(),
        index,
        append,
        value,
    })
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
        assert_eq!(simple.assignments[0].name, "FOO");
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
    fn case_clause() {
        let command = first_command(
            "case $x in\n  a|b) echo ab ;;\n  (*.rs) echo rust ;;\n  *) echo other\nesac",
        );
        let CommandKind::Case { arms, .. } = command.kind else {
            panic!()
        };
        assert_eq!(arms.len(), 3);
        assert_eq!(arms[0].patterns.len(), 2);
    }

    #[test]
    fn empty_case_and_incomplete_case() {
        assert!(parse("case x in esac").is_ok());
        assert!(parse("case x in a) echo").unwrap_err().incomplete);
    }

    #[test]
    fn conditional_expression() {
        let command = first_command("[[ -f $x && ( $y == *.rs || ! -d $z ) ]]");
        let CommandKind::Conditional(Condition::And(left, right)) = command.kind else {
            panic!()
        };
        assert!(matches!(*left, Condition::Unary { .. }));
        assert!(matches!(*right, Condition::Or(..)));
        assert!(parse("[[ a < b ]]").is_ok());
        assert!(parse("[[ -f x").unwrap_err().incomplete);
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

    #[test]
    fn double_negation() {
        assert!(!parse("! ! true").unwrap()[0].first.negate);
        assert!(parse("! ! ! true").unwrap()[0].first.negate);
    }

    #[test]
    fn pipe_with_errors_adds_redirect() {
        let list = parse("ls |& wc").unwrap();
        let commands = &list[0].first.commands;
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].redirects[0].fd, 2);
        assert_eq!(commands[0].redirects[0].kind, RedirectKind::DuplicateOut);
    }

    #[test]
    fn named_redirects() {
        let command = first_command("exec {fd}>out");
        assert_eq!(command.redirects[0].variable.as_deref(), Some("fd"));
        assert_eq!(command.redirects[0].kind, RedirectKind::Out);
    }

    #[test]
    fn option_test_in_conditions() {
        let command = first_command("[[ -o noclobber ]]");
        assert!(matches!(
            command.kind,
            CommandKind::Conditional(Condition::Unary { ref operator, .. }) if operator == "-o"
        ));
    }

    #[test]
    fn nested_groups_inside_conditions() {
        let command = first_command("[[ $a == 1 && (( $a == 1 ) || ( $a == 2 )) ]]");
        assert!(matches!(
            command.kind,
            CommandKind::Conditional(Condition::And(..))
        ));
    }

    #[test]
    fn extended_patterns_inside_conditions() {
        let command = first_command("[[ $x == @(a|b) && $y != +([0-9]) ]]");
        let CommandKind::Conditional(Condition::And(left, _)) = command.kind else {
            panic!()
        };
        let Condition::Binary { right, .. } = *left else {
            panic!()
        };
        assert_eq!(right.as_literal(), Some("@(a|b)"));
    }

    #[test]
    fn case_inside_command_substitution() {
        let command = first_command("echo $( case x in x) echo m;; esac )");
        assert!(matches!(
            simple(&command).words[1].parts[0],
            WordPart::CommandSubstitution { .. }
        ));
    }

    #[test]
    fn select_and_coproc() {
        assert!(matches!(
            first_command("select x in a b; do echo $x; done").kind,
            CommandKind::Select { ref variable, .. } if variable == "x"
        ));
        assert!(matches!(
            first_command("coproc cat").kind,
            CommandKind::Coproc { ref name, .. } if name == "COPROC"
        ));
        assert!(matches!(
            first_command("coproc worker { cat; }").kind,
            CommandKind::Coproc { ref name, .. } if name == "worker"
        ));
    }

    #[test]
    fn units_are_parsed_one_line_at_a_time() {
        let mut parser = Parser::for_text("echo a; echo b\nif x\nthen y\nfi\nfi\n", 1);
        assert_eq!(parser.next_unit().unwrap().unwrap().len(), 2);
        assert_eq!(parser.next_unit().unwrap().unwrap().len(), 1);
        let error = parser.next_unit().unwrap_err();
        assert_eq!(error.line, Some(5));
    }

    #[test]
    fn line_source_is_read_lazily() {
        let lines = std::rc::Rc::new(std::cell::RefCell::new(vec![
            "echo b".to_string(),
            "echo a".to_string(),
        ]));
        let source = std::rc::Rc::clone(&lines);
        let mut parser = Parser::for_lines(Box::new(move || source.borrow_mut().pop()));
        parser.next_unit().unwrap().unwrap();
        assert_eq!(lines.borrow().len(), 1);
    }

    #[test]
    fn final_input_reports_end_of_file() {
        let mut parser = Parser::for_text("if true; then\n", 1);
        let error = parser.next_unit().unwrap_err();
        assert!(!error.incomplete);
        assert!(error.message.contains("end of file"));
    }

    #[test]
    fn nesting_limit_is_a_syntax_error() {
        let deep = format!("{}echo{}", "{ ".repeat(5000), "; }".repeat(5000));
        let result = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || parse(&deep).map(|_| ()))
            .unwrap()
            .join()
            .unwrap();
        assert!(!result.unwrap_err().incomplete);
    }
}
