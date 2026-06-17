use std::borrow::Cow;
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;

use rustyline::Context;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::SearchDirection;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};

use crate::builtins;
use crate::syntax::parser;

const CYAN: &str = "\x1b[36m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const BOLD_RED: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[33m";
const DIM: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

pub struct ShellHelper {
    file_completer: FilenameCompleter,
    commands: Vec<String>,
}

impl ShellHelper {
    pub fn new() -> Self {
        ShellHelper {
            file_completer: FilenameCompleter::new(),
            commands: collect_commands(),
        }
    }

    fn is_valid_command(&self, name: &str) -> bool {
        builtins::is_builtin(name)
            || self
                .commands
                .binary_search_by(|c| c.as_str().cmp(name))
                .is_ok()
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before = &line[..pos];

        let cmd_start = before.rfind(['|', ';', '&']).map(|i| i + 1).unwrap_or(0);

        let fragment = before[cmd_start..].trim_start();

        if fragment.contains(' ') {
            return self.file_completer.complete(line, pos, ctx);
        }

        let word_start = pos - fragment.len();
        let matches = self
            .commands
            .iter()
            .filter(|c| c.starts_with(fragment))
            .map(|c| Pair {
                display: c.clone(),
                replacement: c.clone(),
            })
            .collect();

        Ok((word_start, matches))
    }
}

impl Highlighter for ShellHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        if line.is_empty() {
            return line.into();
        }

        let mut out = String::with_capacity(line.len() + 128);
        let mut expecting_command = true;

        let spans = extract_spans(line);
        for span in &spans {
            match span.kind {
                SpanKind::Whitespace => out.push_str(span.text),
                SpanKind::Comment => color(&mut out, DIM, span.text),
                SpanKind::Operator => {
                    color(&mut out, CYAN, span.text);
                    if matches!(span.text, "|" | "||" | "&&" | ";") {
                        expecting_command = true;
                    }
                }
                SpanKind::SingleQuoted | SpanKind::DoubleQuoted => {
                    color(&mut out, YELLOW, span.text);
                    expecting_command = false;
                }
                SpanKind::Variable | SpanKind::Substitution => {
                    color(&mut out, CYAN, span.text);
                    expecting_command = false;
                }
                SpanKind::Word => {
                    if expecting_command && is_plain_command(span.text) {
                        let c = if self.is_valid_command(span.text) {
                            BOLD_GREEN
                        } else {
                            BOLD_RED
                        };
                        color(&mut out, c, span.text);
                    } else {
                        out.push_str(span.text);
                    }
                    expecting_command = false;
                }
            }
        }

        out.into()
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        format!("{DIM}{hint}{RESET}").into()
    }

    fn highlight_char(
        &self,
        _line: &str,
        _pos: usize,
        _kind: rustyline::highlight::CmdKind,
    ) -> bool {
        true
    }
}

fn color(out: &mut String, ansi: &str, text: &str) {
    out.push_str(ansi);
    out.push_str(text);
    out.push_str(RESET);
}

fn is_plain_command(word: &str) -> bool {
    !word.contains('/') && !word.starts_with('.') && !word.starts_with('~')
}

#[derive(Clone, Copy)]
enum SpanKind {
    Whitespace,
    Operator,
    SingleQuoted,
    DoubleQuoted,
    Variable,
    Substitution,
    Comment,
    Word,
}

struct Span<'a> {
    text: &'a str,
    kind: SpanKind,
}

fn extract_spans(line: &str) -> Vec<Span<'_>> {
    let mut spans = Vec::new();
    let mut chars = line.char_indices().peekable();

    while let Some(&(pos, ch)) = chars.peek() {
        match ch {
            ' ' | '\t' => {
                let start = pos;
                while chars.peek().is_some_and(|&(_, c)| c == ' ' || c == '\t') {
                    chars.next();
                }
                let end = chars.peek().map_or(line.len(), |&(i, _)| i);
                spans.push(Span {
                    text: &line[start..end],
                    kind: SpanKind::Whitespace,
                });
            }
            '#' => {
                spans.push(Span {
                    text: &line[pos..],
                    kind: SpanKind::Comment,
                });
                return spans;
            }
            '|' => {
                chars.next();
                if chars.peek().is_some_and(|&(_, c)| c == '|') {
                    chars.next();
                    spans.push(Span {
                        text: &line[pos..pos + 2],
                        kind: SpanKind::Operator,
                    });
                } else {
                    spans.push(Span {
                        text: &line[pos..pos + 1],
                        kind: SpanKind::Operator,
                    });
                }
            }
            '&' => {
                chars.next();
                if chars.peek().is_some_and(|&(_, c)| c == '&') {
                    chars.next();
                    spans.push(Span {
                        text: &line[pos..pos + 2],
                        kind: SpanKind::Operator,
                    });
                } else {
                    spans.push(Span {
                        text: &line[pos..pos + 1],
                        kind: SpanKind::Operator,
                    });
                }
            }
            ';' | '>' | '<' => {
                chars.next();
                if ch == '>' && chars.peek().is_some_and(|&(_, c)| c == '>') {
                    chars.next();
                    spans.push(Span {
                        text: &line[pos..pos + 2],
                        kind: SpanKind::Operator,
                    });
                } else {
                    spans.push(Span {
                        text: &line[pos..pos + 1],
                        kind: SpanKind::Operator,
                    });
                }
            }
            '\'' => {
                chars.next();
                while let Some(&(_, c)) = chars.peek() {
                    chars.next();
                    if c == '\'' {
                        break;
                    }
                }
                let end = chars.peek().map_or(line.len(), |&(i, _)| i);
                spans.push(Span {
                    text: &line[pos..end],
                    kind: SpanKind::SingleQuoted,
                });
            }
            '"' => {
                chars.next();
                while let Some(&(_, c)) = chars.peek() {
                    chars.next();
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        chars.next();
                    }
                }
                let end = chars.peek().map_or(line.len(), |&(i, _)| i);
                spans.push(Span {
                    text: &line[pos..end],
                    kind: SpanKind::DoubleQuoted,
                });
            }
            '$' => {
                chars.next();
                if chars.peek().is_some_and(|&(_, c)| c == '(') {
                    chars.next();
                    let mut depth = 1;
                    while let Some(&(_, c)) = chars.peek() {
                        chars.next();
                        match c {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let end = chars.peek().map_or(line.len(), |&(i, _)| i);
                    spans.push(Span {
                        text: &line[pos..end],
                        kind: SpanKind::Substitution,
                    });
                } else {
                    while chars
                        .peek()
                        .is_some_and(|&(_, c)| c.is_alphanumeric() || c == '_')
                    {
                        chars.next();
                    }
                    let end = chars.peek().map_or(line.len(), |&(i, _)| i);
                    spans.push(Span {
                        text: &line[pos..end],
                        kind: SpanKind::Variable,
                    });
                }
            }
            '`' => {
                chars.next();
                while let Some(&(_, c)) = chars.peek() {
                    chars.next();
                    if c == '`' {
                        break;
                    }
                }
                let end = chars.peek().map_or(line.len(), |&(i, _)| i);
                spans.push(Span {
                    text: &line[pos..end],
                    kind: SpanKind::Substitution,
                });
            }
            _ => {
                let is_word_char = |c: char| {
                    !matches!(
                        c,
                        ' ' | '\t' | '|' | '&' | ';' | '>' | '<' | '\'' | '"' | '$' | '`' | '#'
                    )
                };
                while let Some(&(_, c)) = chars.peek() {
                    if !is_word_char(c) {
                        break;
                    }
                    chars.next();
                    if c == '\\' {
                        chars.next();
                    }
                }
                let end = chars.peek().map_or(line.len(), |&(i, _)| i);
                spans.push(Span {
                    text: &line[pos..end],
                    kind: SpanKind::Word,
                });
            }
        }
    }

    spans
}

impl Hinter for ShellHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        if line.is_empty() || pos < line.len() {
            return None;
        }

        let history = ctx.history();
        for idx in (0..history.len()).rev() {
            let Ok(Some(entry)) = history.get(idx, SearchDirection::Forward) else {
                continue;
            };
            if entry.entry.starts_with(line) && entry.entry.len() > line.len() {
                return Some(entry.entry[line.len()..].to_string());
            }
        }

        None
    }
}

impl Validator for ShellHelper {
    fn validate(&self, context: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        match parser::parse(context.input()) {
            Err(error) if error.incomplete => Ok(ValidationResult::Incomplete),
            _ => Ok(ValidationResult::Valid(None)),
        }
    }
}
impl rustyline::Helper for ShellHelper {}

fn collect_commands() -> Vec<String> {
    let mut commands = BTreeSet::new();

    for b in builtins::names() {
        commands.insert(b.to_string());
    }

    let Ok(path) = std::env::var("PATH") else {
        return commands.into_iter().collect();
    };

    for dir in path.split(':') {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_file()
                && meta.permissions().mode() & 0o111 != 0
                && let Ok(name) = entry.file_name().into_string()
            {
                commands.insert(name);
            }
        }
    }

    commands.into_iter().collect()
}
