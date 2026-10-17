use std::fmt;

use crate::syntax::ast::{
    Anchor, FallbackKind, ParameterOperation, RedirectKind, Subscript, Word, WordPart,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(Word),
    Operator(&'static str),
    Newline,
    Redirect(i32, RedirectKind),
    Heredoc { fd: i32, index: usize },
    Arithmetic(String),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub incomplete: bool,
    pub line: Option<usize>,
}

impl ParseError {
    pub fn syntax(message: impl Into<String>) -> Self {
        ParseError {
            message: message.into(),
            incomplete: false,
            line: None,
        }
    }

    pub fn incomplete(message: impl Into<String>) -> Self {
        ParseError {
            message: message.into(),
            incomplete: true,
            line: None,
        }
    }
}

impl ParseError {
    pub fn shifted(mut self, first_line: usize) -> Self {
        self.line = Some(self.line.map_or(first_line, |line| line + first_line - 1));
        self
    }

    pub fn at_line(mut self, line: usize) -> Self {
        if self.line.is_none() {
            self.line = Some(line);
        }
        self
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "line {line}: {}", self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

#[derive(Debug)]
pub struct Lexed {
    pub tokens: Vec<Spanned>,
    pub heredocs: Vec<Word>,
}

pub fn lex(source: &str) -> Result<Lexed, ParseError> {
    let mut lexer = Lexer::new(source);
    lexer.run().map_err(|error| {
        let consumed = lexer.position.min(lexer.chars.len());
        let line = lexer.chars[..consumed]
            .iter()
            .filter(|character| **character == '\n')
            .count();
        error.at_line(line + 1)
    })
}

fn escape_bracket_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut quote = None;
    let mut bracket = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (None, '\'' | '"') if !bracket => quote = Some(character),
            (None, '\\') => {
                out.push(character);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
                continue;
            }
            (None, '[') => bracket = true,
            (None, ']') => bracket = false,
            (None, ' ' | '\t') if bracket => out.push('\\'),
            _ => {}
        }
        out.push(character);
    }
    out
}

pub fn parse_regex_word(text: &str) -> Result<Word, ParseError> {
    let escaped = escape_bracket_spaces(text.trim());
    let mut lexer = Lexer::new(&escaped);
    let word = lexer.word_until(|character| character == ' ' || character == '\t')?;
    if lexer.position < lexer.chars.len() {
        return Err(ParseError::syntax("unexpected space in regular expression"));
    }
    Ok(word)
}

pub fn ansi_c(text: &str) -> String {
    Lexer::new(&format!("{}'", text.replace('\'', "\\'")))
        .ansi_quoted()
        .unwrap_or_else(|_| text.to_string())
}

pub fn parse_double_quoted_text(text: &str) -> Result<Word, ParseError> {
    let parts = Lexer::new(text).double_quoted_until(None)?;
    Ok(Word { parts })
}

struct PendingDoc {
    index: usize,
    delim: String,
    strip_tabs: bool,
    expand: bool,
}

struct Lexer {
    chars: Vec<char>,
    position: usize,
    tokens: Vec<Spanned>,
    heredocs: Vec<Word>,
    pending: Vec<PendingDoc>,
}

#[derive(Default)]
struct WordBuf {
    parts: Vec<WordPart>,
}

impl WordBuf {
    fn literal(&mut self, character: char) {
        match self.parts.last_mut() {
            Some(WordPart::Literal(text)) => text.push(character),
            _ => self.parts.push(WordPart::Literal(character.to_string())),
        }
    }

    fn quoted(&mut self, character: char) {
        match self.parts.last_mut() {
            Some(WordPart::Quoted(text)) => text.push(character),
            _ => self.parts.push(WordPart::Quoted(character.to_string())),
        }
    }

    fn quoted_text(&mut self, addition: &str) {
        match self.parts.last_mut() {
            Some(WordPart::Quoted(text)) => text.push_str(addition),
            _ => self.parts.push(WordPart::Quoted(addition.into())),
        }
    }

    fn part(&mut self, part: WordPart) {
        match part {
            WordPart::Literal(text) => text.chars().for_each(|character| self.literal(character)),
            WordPart::Quoted(text) => self.quoted_text(&text),
            other => self.parts.push(other),
        }
    }

    fn finish(self) -> Word {
        Word { parts: self.parts }
    }
}

fn is_name_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

fn is_name_char(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

fn fallback_kind(character: Option<char>) -> FallbackKind {
    match character {
        Some('=') => FallbackKind::Assign,
        Some('+') => FallbackKind::Alternative,
        Some('?') => FallbackKind::Error,
        _ => FallbackKind::Default,
    }
}

fn is_word_break(character: char) -> bool {
    matches!(
        character,
        ' ' | '\t' | '\r' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>'
    )
}

impl Lexer {
    fn new(source: &str) -> Self {
        Lexer {
            chars: source.chars().collect(),
            position: 0,
            tokens: Vec::new(),
            heredocs: Vec::new(),
            pending: Vec::new(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.position + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += 1;
        Some(character)
    }

    fn eat(&mut self, character: char) -> bool {
        if self.peek() == Some(character) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn push(&mut self, token: Token, start: usize) {
        self.tokens.push(Spanned {
            token,
            start,
            end: self.position,
        });
    }

    fn run(&mut self) -> Result<Lexed, ParseError> {
        while let Some(character) = self.peek() {
            let start = self.position;
            match character {
                ' ' | '\t' | '\r' => self.position += 1,
                '\\' if self.peek_at(1) == Some('\n') => self.position += 2,
                '\n' => {
                    self.position += 1;
                    self.push(Token::Newline, start);
                    self.read_heredocs()?;
                }
                '#' => {
                    while self.peek().is_some_and(|character| character != '\n') {
                        self.position += 1;
                    }
                }
                '&' => {
                    self.position += 1;
                    if self.eat('&') {
                        self.push(Token::Operator("&&"), start);
                    } else if self.eat('>') {
                        let kind = if self.eat('>') {
                            RedirectKind::AppendAndError
                        } else {
                            RedirectKind::OutAndError
                        };
                        self.push(Token::Redirect(1, kind), start);
                    } else {
                        self.push(Token::Operator("&"), start);
                    }
                }
                '|' => {
                    self.position += 1;
                    let operator = if self.eat('|') { "||" } else { "|" };
                    self.push(Token::Operator(operator), start);
                }
                ';' => {
                    self.position += 1;
                    let operator = if self.eat(';') {
                        if self.eat('&') { ";;&" } else { ";;" }
                    } else if self.eat('&') {
                        ";&"
                    } else {
                        ";"
                    };
                    self.push(Token::Operator(operator), start);
                }
                '(' if self.peek_at(1) == Some('(') => {
                    self.position += 2;
                    let source = self.scan_arithmetic()?;
                    self.push(Token::Arithmetic(source), start);
                }
                '(' => {
                    self.position += 1;
                    self.push(Token::Operator("("), start);
                }
                ')' => {
                    self.position += 1;
                    self.push(Token::Operator(")"), start);
                }
                '<' | '>' if self.peek_at(1) == Some('(') => {
                    let word = self.word()?;
                    self.push(Token::Word(word), start);
                }
                '<' | '>' => self.redirect(None, start)?,
                character if character.is_ascii_digit() && self.io_number().is_some() => {
                    let fd = self.io_number().unwrap_or(0);
                    while self
                        .peek()
                        .is_some_and(|character| character.is_ascii_digit())
                    {
                        self.position += 1;
                    }
                    self.redirect(Some(fd), start)?;
                }
                _ => {
                    let word = self.word()?;
                    self.push(Token::Word(word), start);
                }
            }
        }

        if !self.pending.is_empty() {
            return Err(ParseError::incomplete("unterminated here-document"));
        }
        let end = self.position;
        self.tokens.push(Spanned {
            token: Token::Eof,
            start: end,
            end,
        });
        Ok(Lexed {
            tokens: std::mem::take(&mut self.tokens),
            heredocs: std::mem::take(&mut self.heredocs),
        })
    }

    fn io_number(&self) -> Option<i32> {
        let mut end = self.position;
        while self
            .chars
            .get(end)
            .is_some_and(|character| character.is_ascii_digit())
        {
            end += 1;
        }
        match self.chars.get(end) {
            Some('<') | Some('>') => self.chars[self.position..end]
                .iter()
                .collect::<String>()
                .parse()
                .ok(),
            _ => None,
        }
    }

    fn redirect(&mut self, fd: Option<i32>, start: usize) -> Result<(), ParseError> {
        let character = self.bump();
        let (default_fd, kind) = if character == Some('>') {
            let kind = if self.eat('>') {
                RedirectKind::Append
            } else if self.eat('&') {
                RedirectKind::DuplicateOut
            } else if self.eat('|') {
                RedirectKind::Clobber
            } else {
                RedirectKind::Out
            };
            (1, kind)
        } else if self.eat('<') {
            if self.eat('<') {
                (0, RedirectKind::HereString)
            } else {
                let strip_tabs = self.eat('-');
                return self.heredoc(fd.unwrap_or(0), strip_tabs, start);
            }
        } else if self.eat('&') {
            (0, RedirectKind::DuplicateIn)
        } else if self.eat('>') {
            (0, RedirectKind::ReadWrite)
        } else {
            (0, RedirectKind::In)
        };
        self.push(Token::Redirect(fd.unwrap_or(default_fd), kind), start);
        Ok(())
    }

    fn heredoc(&mut self, fd: i32, strip_tabs: bool, start: usize) -> Result<(), ParseError> {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.position += 1;
        }
        let word = self.word()?;
        if word.parts.is_empty() {
            return Err(ParseError::syntax(
                "expected here-document delimiter after '<<'",
            ));
        }
        let expand = word
            .parts
            .iter()
            .all(|part| matches!(part, WordPart::Literal(_)));
        let delim = word
            .parts
            .iter()
            .map(|part| match part {
                WordPart::Literal(text) | WordPart::Quoted(text) => text.as_str(),
                _ => "",
            })
            .collect();
        let index = self.heredocs.len();
        self.heredocs.push(Word::default());
        self.pending.push(PendingDoc {
            index,
            delim,
            strip_tabs,
            expand,
        });
        self.push(Token::Heredoc { fd, index }, start);
        Ok(())
    }

    fn read_heredocs(&mut self) -> Result<(), ParseError> {
        for doc in std::mem::take(&mut self.pending) {
            let mut body = String::new();
            loop {
                if self.peek().is_none() {
                    return Err(ParseError::incomplete("unterminated here-document"));
                }
                let mut line = String::new();
                while let Some(character) = self.bump() {
                    if character == '\n' {
                        break;
                    }
                    line.push(character);
                }
                let line = if doc.strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    &line
                };
                if line == doc.delim {
                    break;
                }
                body.push_str(line);
                body.push('\n');
            }
            self.heredocs[doc.index] = if doc.expand {
                parse_double_quoted_text(&body)?
            } else {
                Word {
                    parts: vec![WordPart::Quoted(body)],
                }
            };
        }
        Ok(())
    }

    fn word(&mut self) -> Result<Word, ParseError> {
        self.word_until(is_word_break)
    }

    fn word_until(&mut self, stop: fn(char) -> bool) -> Result<Word, ParseError> {
        let mut word = WordBuf::default();
        while let Some(character) = self.peek() {
            if matches!(character, '<' | '>') && self.peek_at(1) == Some('(') {
                self.position += 2;
                let source = self.scan_substitution()?;
                word.part(WordPart::ProcessSubstitution {
                    source,
                    output: character == '>',
                });
                continue;
            }
            if stop(character) {
                break;
            }
            self.position += 1;
            match character {
                '\\' => match self.bump() {
                    Some('\n') => {}
                    Some(character) => word.quoted(character),
                    None => return Err(ParseError::incomplete("trailing backslash")),
                },
                '\'' => {
                    let text = self.single_quoted()?;
                    word.quoted_text(&text);
                }
                '"' => {
                    let parts = self.double_quoted_until(Some('"'))?;
                    if parts.is_empty() {
                        word.quoted_text("");
                    }
                    parts.into_iter().for_each(|part| word.part(part));
                }
                '$' if self.peek() == Some('\'') => {
                    self.position += 1;
                    let text = self.ansi_quoted()?;
                    word.quoted_text(&text);
                }
                '$' => {
                    let part = self.dollar(false)?;
                    word.part(part);
                }
                '`' => {
                    let source = self.backtick()?;
                    word.part(WordPart::CommandSubstitution {
                        source,
                        quoted: false,
                    });
                }
                '~' if word.parts.is_empty() => {
                    let mut user = String::new();
                    while let Some(character) = self.peek() {
                        if character == '/'
                            || is_word_break(character)
                            || matches!(character, '\'' | '"' | '$' | '`' | '\\')
                        {
                            break;
                        }
                        user.push(character);
                        self.position += 1;
                    }
                    word.part(WordPart::Tilde(user));
                }
                character => word.literal(character),
            }
        }
        Ok(word.finish())
    }

    fn ansi_quoted(&mut self) -> Result<String, ParseError> {
        let mut body = String::new();
        loop {
            let character = self
                .bump()
                .ok_or_else(|| ParseError::incomplete("unclosed $'"))?;
            if character == '\'' {
                return Ok(crate::escapes::expand(&body, crate::escapes::Mode::AnsiC).text);
            }
            body.push(character);
            if character == '\\' {
                let escaped = self
                    .bump()
                    .ok_or_else(|| ParseError::incomplete("unclosed $'"))?;
                body.push(escaped);
            }
        }
    }

    fn single_quoted(&mut self) -> Result<String, ParseError> {
        let mut text = String::new();
        loop {
            match self.bump() {
                Some('\'') => return Ok(text),
                Some(character) => text.push(character),
                None => return Err(ParseError::incomplete("unclosed single quote")),
            }
        }
    }

    fn double_quoted_until(&mut self, end: Option<char>) -> Result<Vec<WordPart>, ParseError> {
        let mut word = WordBuf::default();
        loop {
            match self.bump() {
                None if end.is_none() => break,
                None => return Err(ParseError::incomplete("unclosed double quote")),
                Some(character) if Some(character) == end => break,
                Some('\\') => match self.peek() {
                    Some('\n') => self.position += 1,
                    Some(character)
                        if matches!(character, '$' | '`' | '\\') || Some(character) == end =>
                    {
                        self.position += 1;
                        word.quoted(character);
                    }
                    _ => word.quoted('\\'),
                },
                Some('$') => {
                    let part = self.dollar(true)?;
                    word.part(part);
                }
                Some('`') => {
                    let source = self.backtick()?;
                    word.part(WordPart::CommandSubstitution {
                        source,
                        quoted: true,
                    });
                }
                Some(character) => word.quoted(character),
            }
        }
        Ok(word.parts)
    }

    fn dollar(&mut self, quoted: bool) -> Result<WordPart, ParseError> {
        match self.peek() {
            Some('(') if self.peek_at(1) == Some('(') => {
                self.position += 2;
                let source = self.scan_arithmetic()?;
                let expression = parse_double_quoted_text(&source)?;
                Ok(WordPart::Arithmetic { expression, quoted })
            }
            Some('(') => {
                self.position += 1;
                let source = self.scan_substitution()?;
                Ok(WordPart::CommandSubstitution { source, quoted })
            }
            Some('{') => {
                self.position += 1;
                self.brace_parameter(quoted)
            }
            Some(character) if is_name_start(character) => {
                let name = self.read_name();
                Ok(WordPart::Parameter {
                    name,
                    subscript: None,
                    operator: None,
                    quoted,
                })
            }
            Some(character) if character.is_ascii_digit() || "?$#@*!-".contains(character) => {
                self.position += 1;
                Ok(WordPart::Parameter {
                    name: character.to_string(),
                    subscript: None,
                    operator: None,
                    quoted,
                })
            }
            _ if quoted => Ok(WordPart::Quoted("$".into())),
            _ => Ok(WordPart::Literal("$".into())),
        }
    }

    fn read_name(&mut self) -> String {
        let mut name = String::new();
        while let Some(character) = self.peek().filter(|&character| is_name_char(character)) {
            name.push(character);
            self.position += 1;
        }
        name
    }

    fn brace_parameter(&mut self, quoted: bool) -> Result<WordPart, ParseError> {
        if self.peek() == Some('#') && self.peek_at(1).is_some_and(|character| character != '}') {
            self.position += 1;
            let name = self.parameter_name()?;
            let subscript = self.subscript()?;
            if !self.eat('}') {
                return Err(ParseError::syntax("bad substitution"));
            }
            return Ok(WordPart::Length { name, subscript });
        }
        if self.peek() == Some('!') && self.peek_at(1).is_some_and(is_name_start) {
            self.position += 1;
            let name = self.parameter_name()?;
            if matches!(self.peek(), Some('*' | '@')) && self.peek_at(1) == Some('}') {
                let joined = self.bump() == Some('*');
                self.position += 1;
                return Ok(WordPart::Names {
                    prefix: name,
                    joined,
                    quoted,
                });
            }
            return match self.subscript()? {
                Some(Subscript::All | Subscript::Joined) if self.eat('}') => {
                    Ok(WordPart::Indices { name, quoted })
                }
                None => {
                    let operator = if self.eat('}') {
                        None
                    } else {
                        Some(Box::new(self.parameter_operation()?))
                    };
                    Ok(WordPart::Indirect {
                        name,
                        operator,
                        quoted,
                    })
                }
                Some(_) => Err(ParseError::syntax("bad substitution")),
            };
        }

        let name = self.parameter_name()?;
        let subscript = self.subscript()?;
        let operator = if self.eat('}') {
            None
        } else {
            Some(Box::new(self.parameter_operation()?))
        };
        Ok(WordPart::Parameter {
            name,
            subscript,
            operator,
            quoted,
        })
    }

    fn subscript(&mut self) -> Result<Option<Subscript>, ParseError> {
        if !self.eat('[') {
            return Ok(None);
        }
        let start = self.position;
        let mut depth = 0;
        loop {
            match self.bump() {
                None => return Err(ParseError::incomplete("unclosed ${")),
                Some('[') => depth += 1,
                Some(']') if depth == 0 => break,
                Some(']') => depth -= 1,
                _ => {}
            }
        }
        let text: String = self.chars[start..self.position - 1].iter().collect();
        Ok(Some(match text.trim() {
            "@" => Subscript::All,
            "*" => Subscript::Joined,
            index => Subscript::Index(parse_double_quoted_text(index)?),
        }))
    }

    fn parameter_operation(&mut self) -> Result<ParameterOperation, ParseError> {
        let Some(first) = self.bump() else {
            return Err(ParseError::incomplete("unclosed ${"));
        };
        let operation = match first {
            ':' if matches!(self.peek(), Some('-' | '=' | '+' | '?')) => {
                let kind = fallback_kind(self.bump());
                ParameterOperation::Fallback {
                    kind,
                    colon: true,
                    word: self.brace_word(&['}'])?.0,
                }
            }
            ':' => {
                let (offset, stop) = self.brace_word(&[':', '}'])?;
                let length = if stop == ':' {
                    Some(self.brace_word(&['}'])?.0)
                } else {
                    None
                };
                ParameterOperation::Substring { offset, length }
            }
            '-' | '=' | '+' | '?' => ParameterOperation::Fallback {
                kind: fallback_kind(Some(first)),
                colon: false,
                word: self.brace_word(&['}'])?.0,
            },
            '#' => ParameterOperation::RemovePrefix {
                longest: self.eat('#'),
                pattern: self.brace_word(&['}'])?.0,
            },
            '%' => ParameterOperation::RemoveSuffix {
                longest: self.eat('%'),
                pattern: self.brace_word(&['}'])?.0,
            },
            '/' => {
                let all = self.eat('/');
                let anchor = if all {
                    Anchor::Anywhere
                } else if self.eat('#') {
                    Anchor::Start
                } else if self.eat('%') {
                    Anchor::End
                } else {
                    Anchor::Anywhere
                };
                let (pattern, stop) = self.brace_word(&['/', '}'])?;
                let replacement = if stop == '/' {
                    self.brace_word(&['}'])?.0
                } else {
                    Word::default()
                };
                ParameterOperation::Replace {
                    all,
                    anchor,
                    pattern,
                    replacement,
                }
            }
            '^' | ',' => {
                let all = self.eat(first);
                if !self.eat('}') {
                    return Err(ParseError::syntax("bad substitution"));
                }
                ParameterOperation::ChangeCase {
                    upper: first == '^',
                    all,
                }
            }
            '@' => match (self.bump(), self.bump()) {
                (Some(letter), Some('}')) if "QEULua".contains(letter) => {
                    ParameterOperation::Transform(letter)
                }
                _ => return Err(ParseError::syntax("bad substitution")),
            },
            _ => return Err(ParseError::syntax("bad substitution")),
        };
        Ok(operation)
    }

    fn parameter_name(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Some(character) if is_name_start(character) => Ok(self.read_name()),
            Some(character) if character.is_ascii_digit() => {
                let mut digits = String::new();
                while let Some(digit) = self.peek().filter(|character| character.is_ascii_digit()) {
                    digits.push(digit);
                    self.position += 1;
                }
                Ok(digits)
            }
            Some(character) if "?$#@*!-".contains(character) => {
                self.position += 1;
                Ok(character.to_string())
            }
            None => Err(ParseError::incomplete("unclosed ${")),
            _ => Err(ParseError::syntax("bad substitution")),
        }
    }

    fn brace_word(&mut self, stops: &[char]) -> Result<(Word, char), ParseError> {
        let mut word = WordBuf::default();
        let stop = loop {
            match self.bump() {
                None => return Err(ParseError::incomplete("unclosed ${")),
                Some(character) if stops.contains(&character) => break character,
                Some('\\') => match self.bump() {
                    Some(character) => word.quoted(character),
                    None => return Err(ParseError::incomplete("unclosed ${")),
                },
                Some('\'') => {
                    let text = self.single_quoted()?;
                    word.quoted_text(&text);
                }
                Some('"') => {
                    let parts = self.double_quoted_until(Some('"'))?;
                    if parts.is_empty() {
                        word.quoted_text("");
                    }
                    parts.into_iter().for_each(|part| word.part(part));
                }
                Some('$') if self.peek() == Some('\'') => {
                    self.position += 1;
                    let text = self.ansi_quoted()?;
                    word.quoted_text(&text);
                }
                Some('$') => {
                    let part = self.dollar(false)?;
                    word.part(part);
                }
                Some('`') => {
                    let source = self.backtick()?;
                    word.part(WordPart::CommandSubstitution {
                        source,
                        quoted: false,
                    });
                }
                Some(character) => word.literal(character),
            }
        };
        Ok((word.finish(), stop))
    }

    fn scan_substitution(&mut self) -> Result<String, ParseError> {
        let start = self.position;
        let mut depth = 1;
        loop {
            match self.bump() {
                None => return Err(ParseError::incomplete("unclosed $(")),
                Some('\\') => {
                    self.bump();
                }
                Some('\'') => {
                    self.single_quoted()?;
                }
                Some('"') => self.skip_double_quoted()?,
                Some('(') => depth += 1,
                Some(')') => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(self.chars[start..self.position - 1].iter().collect());
                    }
                }
                _ => {}
            }
        }
    }

    fn skip_double_quoted(&mut self) -> Result<(), ParseError> {
        loop {
            match self.bump() {
                None => return Err(ParseError::incomplete("unclosed double quote")),
                Some('\\') => {
                    self.bump();
                }
                Some('"') => return Ok(()),
                _ => {}
            }
        }
    }

    fn scan_arithmetic(&mut self) -> Result<String, ParseError> {
        let start = self.position;
        let mut depth = 0;
        loop {
            match self.bump() {
                None => return Err(ParseError::incomplete("unclosed $((")),
                Some('\'') => {
                    self.single_quoted()?;
                }
                Some('"') => self.skip_double_quoted()?,
                Some('`') => {
                    self.backtick()?;
                }
                Some('\\') => {
                    self.bump();
                }
                Some('$') if self.peek() == Some('(') && self.peek_at(1) != Some('(') => {
                    self.position += 1;
                    self.scan_substitution()?;
                }
                Some('(') => depth += 1,
                Some(')') if depth > 0 => depth -= 1,
                Some(')') => {
                    if self.eat(')') {
                        return Ok(self.chars[start..self.position - 2].iter().collect());
                    }
                    return Err(ParseError::syntax("expected '))'"));
                }
                _ => {}
            }
        }
    }

    fn backtick(&mut self) -> Result<String, ParseError> {
        let mut source = String::new();
        loop {
            match self.bump() {
                None => return Err(ParseError::incomplete("unclosed backtick")),
                Some('`') => return Ok(source),
                Some('\\') => match self.bump() {
                    Some(character @ ('`' | '\\' | '$')) => source.push(character),
                    Some(character) => {
                        source.push('\\');
                        source.push(character);
                    }
                    None => return Err(ParseError::incomplete("unclosed backtick")),
                },
                Some(character) => source.push(character),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(source: &str) -> Vec<Word> {
        lex(source)
            .unwrap()
            .tokens
            .into_iter()
            .filter_map(|spanned| match spanned.token {
                Token::Word(word) => Some(word),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn plain_words() {
        let word = words("echo hi");
        assert_eq!(word[0].as_literal(), Some("echo"));
        assert_eq!(word[1].as_literal(), Some("hi"));
    }

    #[test]
    fn quoting_produces_quoted_parts() {
        let word = words("a'b c'\"d\"");
        assert_eq!(
            word[0].parts,
            vec![
                WordPart::Literal("a".into()),
                WordPart::Quoted("b cd".into())
            ]
        );
    }

    #[test]
    fn empty_quotes_make_a_word() {
        let word = words("''");
        assert_eq!(word[0].parts, vec![WordPart::Quoted(String::new())]);
    }

    #[test]
    fn params_are_not_expanded() {
        let word = words("$HOME \"$1\" ${x:-y}");
        assert!(
            matches!(&word[0].parts[0], WordPart::Parameter { name, quoted: false, .. } if name == "HOME")
        );
        assert!(
            matches!(&word[1].parts[0], WordPart::Parameter { name, quoted: true, .. } if name == "1")
        );
        assert!(matches!(
            &word[2].parts[0],
            WordPart::Parameter {
                operator: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn command_substitution_is_kept_as_source() {
        let word = words("$(echo $(date) ')')");
        assert_eq!(
            word[0].parts,
            vec![WordPart::CommandSubstitution {
                source: "echo $(date) ')'".into(),
                quoted: false
            }]
        );
    }

    #[test]
    fn ansi_c_quoting() {
        let word = words("$'tab\\there\\x41\\101\\'q'");
        assert_eq!(
            word[0].parts,
            vec![WordPart::Quoted("tab\there\u{41}A'q".into())]
        );
    }

    #[test]
    fn arithmetic_with_nested_substitution() {
        let word = words("$(($(echo ')' | wc -c) / 2))");
        assert!(matches!(word[0].parts[0], WordPart::Arithmetic { .. }));
    }

    #[test]
    fn io_numbers() {
        let tokens = lex("cmd 2>&1 3<in").unwrap().tokens;
        assert!(
            tokens
                .iter()
                .any(|spanned| spanned.token == Token::Redirect(2, RedirectKind::DuplicateOut))
        );
        assert!(
            tokens
                .iter()
                .any(|spanned| spanned.token == Token::Redirect(3, RedirectKind::In))
        );
    }

    #[test]
    fn heredoc_body() {
        let lexed = lex("cat <<EOF\nhi $X\nEOF\n").unwrap();
        assert_eq!(lexed.heredocs.len(), 1);
        assert!(
            matches!(&lexed.heredocs[0].parts[1], WordPart::Parameter { name, .. } if name == "X")
        );
    }

    #[test]
    fn quoted_heredoc_is_literal() {
        let lexed = lex("cat <<'EOF'\nhi $X\nEOF\n").unwrap();
        assert_eq!(
            lexed.heredocs[0].parts,
            vec![WordPart::Quoted("hi $X\n".into())]
        );
    }

    #[test]
    fn unterminated_things_are_incomplete() {
        for source in ["echo 'a", "echo \"a", "echo $(a", "cat <<EOF\nx", "echo \\"] {
            assert!(lex(source).unwrap_err().incomplete, "{source}");
        }
    }
}
