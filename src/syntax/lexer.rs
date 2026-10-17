use std::fmt;

use crate::syntax::ast::{
    Anchor, FallbackKind, ParameterOperation, RedirectKind, Subscript, Word, WordPart,
};

pub const MAX_NESTING: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(Word),
    Operator(&'static str),
    Newline,
    Redirect(i32, RedirectKind),
    NamedRedirect(String, RedirectKind),
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

    pub fn too_deep() -> Self {
        ParseError::syntax("maximum nesting depth exceeded")
    }
}

impl ParseError {
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
}

pub fn lex(source: &str) -> Result<Lexed, ParseError> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let spanned = lexer.next_token().map_err(|error| {
            let line = lexer.line_at(lexer.position);
            error.at_line(line)
        })?;
        let done = spanned.token == Token::Eof;
        tokens.push(spanned);
        if done {
            break;
        }
    }
    Ok(Lexed { tokens })
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
    line: usize,
}

pub type LineSource<'a> = Box<dyn FnMut() -> Option<String> + 'a>;

pub struct Lexer<'a> {
    chars: Vec<char>,
    position: usize,
    newlines: Vec<usize>,
    first_line: usize,
    heredocs: Vec<Word>,
    pending: Vec<PendingDoc>,
    more: Option<LineSource<'a>>,
    pub(crate) final_input: bool,
    pub(crate) extglob: bool,
    pub(crate) condition: bool,
    pub(crate) depth: usize,
    pub(crate) warnings: Vec<ParseError>,
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
        ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>'
    )
}

fn newline_positions(chars: &[char], offset: usize) -> impl Iterator<Item = usize> + '_ {
    chars
        .iter()
        .enumerate()
        .filter(|(_, character)| **character == '\n')
        .map(move |(index, _)| index + offset)
}

impl<'a> Lexer<'a> {
    pub fn new(source: &str) -> Self {
        let chars: Vec<char> = source.chars().collect();
        let newlines = newline_positions(&chars, 0).collect();
        Lexer {
            chars,
            position: 0,
            newlines,
            first_line: 1,
            heredocs: Vec::new(),
            pending: Vec::new(),
            more: None,
            final_input: false,
            extglob: crate::expand::glob::extglob(),
            condition: false,
            depth: 0,
            warnings: Vec::new(),
        }
    }

    pub fn from_lines(source: LineSource<'a>) -> Self {
        let mut lexer = Lexer::new("");
        lexer.more = Some(source);
        lexer
    }

    pub fn starting_at_line(mut self, first_line: usize) -> Self {
        self.first_line = first_line;
        self
    }

    pub(crate) fn position(&self) -> usize {
        self.position
    }

    pub(crate) fn text(&self, start: usize, end: usize) -> String {
        let end = end.min(self.chars.len());
        self.chars[start.min(end)..end].iter().collect()
    }

    pub(crate) fn line_at(&self, index: usize) -> usize {
        self.first_line + self.newlines.partition_point(|&newline| newline < index)
    }

    pub(crate) fn has_pending_heredocs(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn heredoc_body(&self, index: usize) -> Word {
        self.heredocs.get(index).cloned().unwrap_or_default()
    }

    pub(crate) fn release_consumed(&mut self) {
        if self.more.is_some() && self.pending.is_empty() && self.position == self.chars.len() {
            self.first_line = self.line_at(self.position);
            self.chars.clear();
            self.newlines.clear();
            self.position = 0;
        }
    }

    fn fill(&mut self, offset: usize) -> bool {
        while self.position + offset >= self.chars.len() {
            let Some(more) = self.more.as_mut() else {
                return false;
            };
            let Some(mut line) = more() else {
                self.more = None;
                return false;
            };
            if !line.ends_with('\n') {
                line.push('\n');
            }
            let offset = self.chars.len();
            let added: Vec<char> = line.chars().collect();
            self.newlines.extend(newline_positions(&added, offset));
            self.chars.extend(added);
        }
        true
    }

    fn peek(&mut self) -> Option<char> {
        if self.position >= self.chars.len() && !self.fill(0) {
            return None;
        }
        self.chars.get(self.position).copied()
    }

    fn peek_at(&mut self, offset: usize) -> Option<char> {
        if self.position + offset >= self.chars.len() && !self.fill(offset) {
            return None;
        }
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

    fn enter(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_NESTING || crate::stack::exhausted() {
            self.depth -= 1;
            return Err(ParseError::too_deep());
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    pub fn next_token(&mut self) -> Result<Spanned, ParseError> {
        loop {
            let Some(character) = self.peek() else {
                return self.end_of_input();
            };
            let start = self.position;
            let token = match character {
                ' ' | '\t' => {
                    self.position += 1;
                    continue;
                }
                '\\' if self.peek_at(1) == Some('\n') => {
                    self.position += 2;
                    continue;
                }
                '\n' => {
                    self.position += 1;
                    let spanned = Spanned {
                        token: Token::Newline,
                        start,
                        end: self.position,
                    };
                    self.read_heredocs()?;
                    return Ok(spanned);
                }
                '#' => {
                    while self.peek().is_some_and(|character| character != '\n') {
                        self.position += 1;
                    }
                    continue;
                }
                '&' => {
                    self.position += 1;
                    if self.eat('&') {
                        Token::Operator("&&")
                    } else if self.eat('>') {
                        let kind = if self.eat('>') {
                            RedirectKind::AppendAndError
                        } else {
                            RedirectKind::OutAndError
                        };
                        Token::Redirect(1, kind)
                    } else {
                        Token::Operator("&")
                    }
                }
                '|' => {
                    self.position += 1;
                    if self.eat('|') {
                        Token::Operator("||")
                    } else if self.eat('&') {
                        Token::Operator("|&")
                    } else {
                        Token::Operator("|")
                    }
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
                    Token::Operator(operator)
                }
                '(' if self.peek_at(1) == Some('(') && !self.condition => {
                    self.position += 2;
                    match self.scan_arithmetic() {
                        Ok(source) => Token::Arithmetic(source),
                        Err(error) if error.incomplete => return Err(error),
                        Err(_) => {
                            self.position = start + 1;
                            Token::Operator("(")
                        }
                    }
                }
                '(' => {
                    self.position += 1;
                    Token::Operator("(")
                }
                ')' => {
                    self.position += 1;
                    Token::Operator(")")
                }
                '<' | '>' if self.peek_at(1) == Some('(') => Token::Word(self.word()?),
                '<' | '>' => self.redirect(None)?,
                character if character.is_ascii_digit() && self.io_number().is_some() => {
                    let fd = self.io_number().unwrap_or(0);
                    while self
                        .peek()
                        .is_some_and(|character| character.is_ascii_digit())
                    {
                        self.position += 1;
                    }
                    self.redirect(Some(fd))?
                }
                '{' if self.named_fd().is_some() => {
                    let name = self.named_fd().unwrap_or_default();
                    self.position += name.chars().count() + 2;
                    match self.redirect(None)? {
                        Token::Redirect(_, kind) => Token::NamedRedirect(name, kind),
                        other => other,
                    }
                }
                _ => Token::Word(self.word()?),
            };
            return Ok(Spanned {
                token,
                start,
                end: self.position,
            });
        }
    }

    fn end_of_input(&mut self) -> Result<Spanned, ParseError> {
        if !self.pending.is_empty() {
            if !self.final_input {
                return Err(ParseError::incomplete("unterminated here-document"));
            }
            self.read_heredocs()?;
        }
        let end = self.position;
        Ok(Spanned {
            token: Token::Eof,
            start: end,
            end,
        })
    }

    fn named_fd(&mut self) -> Option<String> {
        let mut offset = 1;
        let mut name = String::new();
        let mut subscript = false;
        while let Some(character) = self.peek_at(offset) {
            if character == '}' || character == '\n' {
                break;
            }
            let valid = if subscript {
                true
            } else if name.is_empty() {
                is_name_start(character)
            } else {
                is_name_char(character) || character == '['
            };
            if !valid {
                return None;
            }
            subscript = subscript || character == '[';
            name.push(character);
            offset += 1;
        }
        if subscript && !name.ends_with(']') {
            return None;
        }
        if name.is_empty() || self.peek_at(offset) != Some('}') {
            return None;
        }
        match (self.peek_at(offset + 1), self.peek_at(offset + 2)) {
            (Some('<'), Some('<' | '(')) | (Some('>'), Some('(')) => None,
            (Some('<' | '>'), _) => Some(name),
            _ => None,
        }
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

    fn redirect(&mut self, fd: Option<i32>) -> Result<Token, ParseError> {
        let start = self.position;
        let character = self.bump();
        let (default_fd, kind) = if character == Some('>') {
            let kind = if self.eat('>') {
                RedirectKind::Append
            } else if self.eat('&') {
                RedirectKind::DuplicateOut
            } else {
                self.eat('|');
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
        Ok(Token::Redirect(fd.unwrap_or(default_fd), kind))
    }

    fn heredoc(&mut self, fd: i32, strip_tabs: bool, start: usize) -> Result<Token, ParseError> {
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
        let line = self.line_at(start);
        self.pending.push(PendingDoc {
            index,
            delim,
            strip_tabs,
            expand,
            line,
        });
        Ok(Token::Heredoc { fd, index })
    }

    fn read_heredocs(&mut self) -> Result<(), ParseError> {
        for doc in std::mem::take(&mut self.pending) {
            let mut body = String::new();
            loop {
                if self.peek().is_none() {
                    if !self.final_input {
                        return Err(ParseError::incomplete("unterminated here-document"));
                    }
                    let line = self.line_at(self.position.saturating_sub(1));
                    self.warnings.push(
                        ParseError::syntax(format!(
                            "warning: here-document at line {} delimited by end-of-file (wanted `{}')",
                            doc.line, doc.delim
                        ))
                        .at_line(line),
                    );
                    break;
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

    fn starts_extglob(&mut self, character: char) -> bool {
        (self.extglob || self.condition)
            && matches!(character, '@' | '?' | '*' | '+' | '!')
            && self.peek_at(1) == Some('(')
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
            if self.starts_extglob(character) {
                self.extglob_group(&mut word)?;
                continue;
            }
            if stop(character) {
                break;
            }
            self.position += 1;
            if character == '~' && word.parts.is_empty() {
                let user = self.tilde_user();
                word.part(WordPart::Tilde(user));
                continue;
            }
            self.word_character(character, &mut word)?;
        }
        Ok(word.finish())
    }

    fn tilde_user(&mut self) -> String {
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
        user
    }

    fn word_character(&mut self, character: char, word: &mut WordBuf) -> Result<(), ParseError> {
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
            '$' if self.peek() == Some('"') => {}
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
            character => word.literal(character),
        }
        Ok(())
    }

    fn extglob_group(&mut self, word: &mut WordBuf) -> Result<(), ParseError> {
        let mut depth = 0usize;
        while let Some(character) = self.bump() {
            match character {
                '(' => {
                    depth += 1;
                    word.literal(character);
                }
                ')' => {
                    word.literal(character);
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                '|' | ' ' | '\t' | '\n' | ';' | '&' | '<' | '>' => word.literal(character),
                _ => self.word_character(character, word)?,
            }
        }
        Err(ParseError::incomplete("unclosed extended pattern"))
    }

    fn ansi_quoted(&mut self) -> Result<String, ParseError> {
        let mut text = String::new();
        loop {
            let character = self
                .bump()
                .ok_or_else(|| ParseError::incomplete("unclosed $'"))?;
            if character == '\'' {
                return Ok(text);
            }
            if character != '\\' {
                text.push(character);
                continue;
            }
            let escaped = self
                .bump()
                .ok_or_else(|| ParseError::incomplete("unclosed $'"))?;
            match escaped {
                'n' => text.push('\n'),
                't' => text.push('\t'),
                'r' => text.push('\r'),
                'a' => text.push('\x07'),
                'b' => text.push('\x08'),
                'e' | 'E' => text.push('\x1b'),
                'f' => text.push('\x0c'),
                'v' => text.push('\x0b'),
                'x' => text.push(self.escape_digits(16, 2, None)),
                'u' => text.push(self.escape_digits(16, 4, None)),
                'U' => text.push(self.escape_digits(16, 8, None)),
                '0'..='7' => text.push(self.escape_digits(8, 3, Some(escaped))),
                'c' => {
                    if let Some(control) = self.bump() {
                        text.push(char::from((control.to_ascii_uppercase() as u8) ^ 0x40));
                    }
                }
                other => text.push(other),
            }
        }
    }

    fn escape_digits(&mut self, radix: u32, limit: usize, first: Option<char>) -> char {
        let mut value = first.and_then(|digit| digit.to_digit(radix)).unwrap_or(0);
        let mut count = usize::from(first.is_some());
        while count < limit
            && let Some(digit) = self.peek().and_then(|next| next.to_digit(radix))
        {
            value = value * radix + digit;
            self.position += 1;
            count += 1;
        }
        char::from_u32(value).unwrap_or('\u{fffd}')
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
        self.enter()?;
        let result = self.dollar_inner(quoted);
        self.leave();
        result
    }

    fn dollar_inner(&mut self, quoted: bool) -> Result<WordPart, ParseError> {
        match self.peek() {
            Some('(') if self.peek_at(1) == Some('(') => {
                let open = self.position;
                self.position += 2;
                match self.scan_arithmetic() {
                    Ok(source) => {
                        let expression = parse_double_quoted_text(&source)?;
                        Ok(WordPart::Arithmetic { expression, quoted })
                    }
                    Err(error) if error.incomplete => Err(error),
                    Err(_) => {
                        self.position = open + 1;
                        let source = self.scan_substitution()?;
                        Ok(WordPart::CommandSubstitution { source, quoted })
                    }
                }
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
        let end = crate::syntax::parser::substitution_end(self)?;
        let source = self.text(start, end);
        self.position = end + 1;
        Ok(source)
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
                    self.enter()?;
                    let scanned = self.scan_substitution();
                    self.leave();
                    scanned?;
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
    fn command_substitution_with_case_and_comment() {
        let word = words("$( case x in x) echo m;; esac )");
        assert_eq!(
            word[0].parts,
            vec![WordPart::CommandSubstitution {
                source: " case x in x) echo m;; esac ".into(),
                quoted: false
            }]
        );
        let word = words("$(echo a # )\n)");
        assert!(matches!(
            &word[0].parts[0],
            WordPart::CommandSubstitution { source, .. } if source == "echo a # )\n"
        ));
    }

    #[test]
    fn arithmetic_falls_back_to_nested_subshells() {
        let word = words("$((echo a); (echo b))");
        assert!(matches!(
            &word[0].parts[0],
            WordPart::CommandSubstitution { source, .. } if source == "(echo a); (echo b)"
        ));
        let tokens = lex("((echo a) )").unwrap().tokens;
        assert_eq!(tokens[0].token, Token::Operator("("));
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
    fn locale_strings_are_double_quoted() {
        let word = words("$\"hi $x\"");
        assert_eq!(word[0].parts[0], WordPart::Quoted("hi ".into()));
        assert!(matches!(&word[0].parts[1], WordPart::Parameter { name, .. } if name == "x"));
    }

    #[test]
    fn carriage_returns_are_literal() {
        let word = words("echo hi\r\n");
        assert_eq!(word[1].as_literal(), Some("hi\r"));
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
    fn named_file_descriptors_and_pipe_with_errors() {
        let tokens = lex("exec {fd}>file {in}<x a |& b {x}").unwrap().tokens;
        assert!(
            tokens.iter().any(
                |spanned| spanned.token == Token::NamedRedirect("fd".into(), RedirectKind::Out)
            )
        );
        assert!(tokens.iter().any(
            |spanned| spanned.token == Token::NamedRedirect("in".into(), RedirectKind::In)
        ));
        assert!(
            tokens
                .iter()
                .any(|spanned| spanned.token == Token::Operator("|&"))
        );
        assert!(tokens.iter().any(|spanned| matches!(&spanned.token,
            Token::Word(word) if word.as_literal() == Some("{x}"))));
    }

    #[test]
    fn extended_patterns_in_conditions() {
        let mut lexer = Lexer::new("@(a|b c) +(x)");
        lexer.condition = true;
        let first = lexer.next_token().unwrap();
        assert!(
            matches!(first.token, Token::Word(ref word) if word.as_literal() == Some("@(a|b c)"))
        );
        let mut lexer = Lexer::new("@(a|b)");
        lexer.extglob = true;
        assert!(matches!(lexer.next_token().unwrap().token, Token::Word(_)));
    }

    fn heredocs(source: &str) -> Vec<Word> {
        let mut lexer = Lexer::new(source);
        while lexer.next_token().unwrap().token != Token::Eof {}
        lexer.heredocs
    }

    #[test]
    fn heredoc_body() {
        let heredocs = heredocs("cat <<EOF\nhi $X\nEOF\n");
        assert_eq!(heredocs.len(), 1);
        assert!(matches!(&heredocs[0].parts[1], WordPart::Parameter { name, .. } if name == "X"));
    }

    #[test]
    fn heredoc_keeps_blank_lines() {
        assert_eq!(
            heredocs("cat <<'EOF'\na\n\nb\nEOF\n")[0].parts,
            vec![WordPart::Quoted("a\n\nb\n".into())]
        );
    }

    #[test]
    fn quoted_heredoc_is_literal() {
        assert_eq!(
            heredocs("cat <<'EOF'\nhi $X\nEOF\n")[0].parts,
            vec![WordPart::Quoted("hi $X\n".into())]
        );
    }

    #[test]
    fn heredoc_at_end_of_final_input_warns() {
        let mut lexer = Lexer::new("cat <<EOF\na\n");
        lexer.final_input = true;
        while lexer.next_token().unwrap().token != Token::Eof {}
        assert_eq!(lexer.warnings.len(), 1);
        assert!(lexer.warnings[0].message.contains("wanted `EOF'"));
        assert_eq!(
            lexer.heredoc_body(0).parts,
            vec![WordPart::Quoted("a\n".into())]
        );
    }

    #[test]
    fn lines_are_pulled_on_demand() {
        let mut lines = vec!["echo 'a".to_string(), "b'".to_string()].into_iter();
        let mut lexer = Lexer::from_lines(Box::new(move || lines.next()));
        lexer.next_token().unwrap();
        let word = lexer.next_token().unwrap();
        assert!(
            matches!(word.token, Token::Word(ref word) if word.parts == vec![WordPart::Quoted("a\nb".into())])
        );
    }

    #[test]
    fn unterminated_things_are_incomplete() {
        for source in ["echo 'a", "echo \"a", "echo $(a", "cat <<EOF\nx", "echo \\"] {
            assert!(lex(source).unwrap_err().incomplete, "{source}");
        }
    }
}
