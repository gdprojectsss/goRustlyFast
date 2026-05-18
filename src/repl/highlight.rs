const CYAN: &str = "\x1b[36m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const BOLD_RED: &str = "\x1b[1;31m";
const MAGENTA: &str = "\x1b[35m";
const YELLOW: &str = "\x1b[33m";
pub(super) const DIM: &str = "\x1b[90m";
pub(super) const RESET: &str = "\x1b[0m";

const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "while", "until", "for", "in", "do", "done", "function",
    "{", "}", "!",
];

pub(super) fn highlight(line: &str, is_command: impl Fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(line.len() + 128);
    let mut expecting_command = true;

    for span in &extract_spans(line) {
        match span.kind {
            SpanKind::Whitespace => out.push_str(span.text),
            SpanKind::Comment => color(&mut out, DIM, span.text),
            SpanKind::Operator => {
                color(&mut out, CYAN, span.text);
                if matches!(span.text, "|" | "||" | "&&" | ";" | "&") {
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
            SpanKind::Word if expecting_command && KEYWORDS.contains(&span.text) => {
                color(&mut out, MAGENTA, span.text);
                expecting_command = !matches!(span.text, "for" | "function" | "in");
            }
            SpanKind::Word => {
                if expecting_command && is_plain_command(span.text) {
                    let shade = if is_command(span.text) {
                        BOLD_GREEN
                    } else {
                        BOLD_RED
                    };
                    color(&mut out, shade, span.text);
                } else {
                    out.push_str(span.text);
                }
                expecting_command = false;
            }
        }
    }

    out
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(text: &str) -> String {
        let mut out = String::new();
        let mut in_escape = false;
        for character in text.chars() {
            match character {
                '\x1b' => in_escape = true,
                'm' if in_escape => in_escape = false,
                _ if in_escape => {}
                _ => out.push(character),
            }
        }
        out
    }

    #[test]
    fn highlighting_keeps_the_text() {
        let line = "if true; then echo \"$HOME\" | grep x; fi # note";
        assert_eq!(strip(&highlight(line, |_| true)), line);
    }

    #[test]
    fn keywords_and_unknown_commands_are_colored() {
        let colored = highlight("if nope", |name| name != "nope");
        assert!(colored.contains(&format!("{MAGENTA}if{RESET}")));
        assert!(colored.contains(&format!("{BOLD_RED}nope{RESET}")));
    }
}
