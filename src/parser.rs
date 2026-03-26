use std::iter::Peekable;
use std::str::Chars;

pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    pub stdin_file: Option<String>,
    pub stdout_file: Option<String>,
    pub append: bool,
}

pub struct Pipeline {
    pub commands: Vec<Command>,
}

#[derive(Clone, Copy)]
pub enum Connector {
    And,
    Or,
    Seq,
}

pub struct CommandLine {
    pub first: Pipeline,
    pub rest: Vec<(Connector, Pipeline)>,
}

enum Token {
    Word(String),
    Pipe,
    And,
    Or,
    Semi,
    RedirectOut,
    RedirectAppend,
    RedirectIn,
}

pub fn parse(input: &str) -> Result<CommandLine, String> {
    let tokens = tokenize(input)?;

    let mut groups: Vec<Vec<Token>> = vec![Vec::new()];
    let mut connectors: Vec<Connector> = Vec::new();

    for token in tokens {
        match token {
            Token::And => {
                connectors.push(Connector::And);
                groups.push(Vec::new());
            }
            Token::Or => {
                connectors.push(Connector::Or);
                groups.push(Vec::new());
            }
            Token::Semi => {
                connectors.push(Connector::Seq);
                groups.push(Vec::new());
            }
            other => groups.last_mut().unwrap().push(other),
        }
    }

    if groups.last().is_some_and(|g| g.is_empty()) {
        groups.pop();
        connectors.pop();
    }

    if groups.is_empty() {
        return Err("empty command".into());
    }

    let mut pipelines: Vec<Pipeline> = groups
        .into_iter()
        .map(structure_pipeline)
        .collect::<Result<_, _>>()?;

    let first = pipelines.remove(0);
    let rest = connectors.into_iter().zip(pipelines).collect();

    Ok(CommandLine { first, rest })
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' => {
                chars.next();
            }
            '#' => break,
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(Token::Or);
                } else {
                    tokens.push(Token::Pipe);
                }
            }
            '&' => {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    tokens.push(Token::And);
                } else {
                    return Err("syntax error near '&'".into());
                }
            }
            ';' => {
                chars.next();
                tokens.push(Token::Semi);
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(Token::RedirectAppend);
                } else {
                    tokens.push(Token::RedirectOut);
                }
            }
            '<' => {
                chars.next();
                tokens.push(Token::RedirectIn);
            }
            _ => {
                tokens.push(Token::Word(read_word(&mut chars)?));
            }
        }
    }

    Ok(tokens)
}

fn read_word(chars: &mut Peekable<Chars>) -> Result<String, String> {
    let mut word = String::new();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '|' | '<' | '>' | '&' | ';' | '#' => break,
            '\'' => {
                chars.next();
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err("unclosed single quote".into()),
                    }
                }
            }
            '"' => {
                chars.next();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c) => word.push(c),
                            None => return Err("unclosed double quote".into()),
                        },
                        Some('$') => word.push_str(&expand_var(chars)),
                        Some(c) => word.push(c),
                        None => return Err("unclosed double quote".into()),
                    }
                }
            }
            '\\' => {
                chars.next();
                if let Some(c) = chars.next() {
                    word.push(c);
                }
            }
            '$' => {
                chars.next();
                word.push_str(&expand_var(chars));
            }
            '~' if word.is_empty() => {
                chars.next();
                match std::env::var("HOME") {
                    Ok(home) => word.push_str(&home),
                    Err(_) => word.push('~'),
                }
            }
            _ => {
                chars.next();
                word.push(ch);
            }
        }
    }

    Ok(word)
}

fn expand_var(chars: &mut Peekable<Chars>) -> String {
    let mut name = String::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_alphanumeric() || ch == '_' {
            name.push(ch);
            chars.next();
        } else {
            break;
        }
    }

    if name.is_empty() {
        return "$".into();
    }

    std::env::var(&name).unwrap_or_default()
}

fn structure_pipeline(tokens: Vec<Token>) -> Result<Pipeline, String> {
    let mut commands = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut stdin_file = None;
    let mut stdout_file = None;
    let mut append = false;

    let mut iter = tokens.into_iter();

    while let Some(token) = iter.next() {
        match token {
            Token::Word(w) => words.push(w),
            Token::Pipe => {
                if words.is_empty() {
                    return Err("syntax error near '|'".into());
                }
                let program = words.remove(0);
                commands.push(Command {
                    program,
                    args: std::mem::take(&mut words),
                    stdin_file: stdin_file.take(),
                    stdout_file: stdout_file.take(),
                    append,
                });
                append = false;
            }
            Token::RedirectOut => match iter.next() {
                Some(Token::Word(file)) => {
                    stdout_file = Some(file);
                    append = false;
                }
                _ => return Err("expected filename after '>'".into()),
            },
            Token::RedirectAppend => match iter.next() {
                Some(Token::Word(file)) => {
                    stdout_file = Some(file);
                    append = true;
                }
                _ => return Err("expected filename after '>>'".into()),
            },
            Token::RedirectIn => match iter.next() {
                Some(Token::Word(file)) => stdin_file = Some(file),
                _ => return Err("expected filename after '<'".into()),
            },
            Token::And | Token::Or | Token::Semi => {
                return Err("unexpected connector in pipeline".into());
            }
        }
    }

    if words.is_empty() {
        if commands.is_empty() {
            return Err("empty command".into());
        }
        return Err("syntax error near '|'".into());
    }

    let program = words.remove(0);
    commands.push(Command {
        program,
        args: words,
        stdin_file,
        stdout_file,
        append,
    });

    Ok(Pipeline { commands })
}
