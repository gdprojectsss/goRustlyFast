use std::iter::Peekable;
use std::process::Stdio;
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
                let (word, has_glob) = read_word(&mut chars)?;
                if has_glob {
                    match crate::glob::expand(&word) {
                        Some(matches) => {
                            for m in matches {
                                tokens.push(Token::Word(m));
                            }
                        }
                        None => tokens.push(Token::Word(word)),
                    }
                } else {
                    tokens.push(Token::Word(word));
                }
            }
        }
    }

    Ok(tokens)
}

fn read_word(chars: &mut Peekable<Chars>) -> Result<(String, bool), String> {
    let mut word = String::new();
    let mut has_glob = false;

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
                        Some('$') => word.push_str(&handle_dollar(chars)),
                        Some('`') => word.push_str(&read_backtick(chars)?),
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
                word.push_str(&handle_dollar(chars));
            }
            '`' => {
                chars.next();
                word.push_str(&read_backtick(chars)?);
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
                if ch == '*' || ch == '?' {
                    has_glob = true;
                }
                word.push(ch);
            }
        }
    }

    Ok((word, has_glob))
}

fn handle_dollar(chars: &mut Peekable<Chars>) -> String {
    if chars.peek() == Some(&'(') {
        chars.next();
        match read_subcommand(chars) {
            Ok(subcmd) => capture_command(&subcmd),
            Err(_) => String::new(),
        }
    } else {
        expand_var(chars)
    }
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

fn read_subcommand(chars: &mut Peekable<Chars>) -> Result<String, String> {
    let mut cmd = String::new();
    let mut depth = 1;
    loop {
        match chars.next() {
            Some('(') => {
                depth += 1;
                cmd.push('(');
            }
            Some(')') => {
                depth -= 1;
                if depth == 0 {
                    return Ok(cmd);
                }
                cmd.push(')');
            }
            Some(c) => cmd.push(c),
            None => return Err("unclosed $(".into()),
        }
    }
}

fn read_backtick(chars: &mut Peekable<Chars>) -> Result<String, String> {
    let mut cmd = String::new();
    loop {
        match chars.next() {
            Some('`') => return Ok(capture_command(&cmd)),
            Some(c) => cmd.push(c),
            None => return Err("unclosed backtick".into()),
        }
    }
}

fn capture_command(cmd: &str) -> String {
    std::process::Command::new("sh")
        .args(["-c", cmd])
        .stderr(Stdio::inherit())
        .output()
        .ok()
        .map(|o| {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            while s.ends_with('\n') {
                s.pop();
            }
            s
        })
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn first_cmd(input: &str) -> Command {
        let cl = parse(input).unwrap();
        assert!(cl.rest.is_empty());
        assert_eq!(cl.first.commands.len(), 1);
        cl.first.commands.into_iter().next().unwrap()
    }

    #[test]
    fn simple_command() {
        let cmd = first_cmd("ls -la /tmp");
        assert_eq!(cmd.program, "ls");
        assert_eq!(cmd.args, ["-la", "/tmp"]);
    }

    #[test]
    fn single_quoted_arg() {
        let cmd = first_cmd("echo 'hello world'");
        assert_eq!(cmd.args, ["hello world"]);
    }

    #[test]
    fn double_quoted_arg() {
        let cmd = first_cmd("echo \"hello world\"");
        assert_eq!(cmd.args, ["hello world"]);
    }

    #[test]
    fn backslash_escape() {
        let cmd = first_cmd("echo hello\\ world");
        assert_eq!(cmd.program, "echo");
        assert_eq!(cmd.args, ["hello world"]);
    }

    #[test]
    fn stdout_redirect() {
        let cmd = first_cmd("echo hello > out.txt");
        assert_eq!(cmd.program, "echo");
        assert_eq!(cmd.args, ["hello"]);
        assert_eq!(cmd.stdout_file.as_deref(), Some("out.txt"));
        assert!(!cmd.append);
    }

    #[test]
    fn stdout_append() {
        let cmd = first_cmd("echo hello >> out.txt");
        assert_eq!(cmd.stdout_file.as_deref(), Some("out.txt"));
        assert!(cmd.append);
    }

    #[test]
    fn stdin_redirect() {
        let cmd = first_cmd("cat < input.txt");
        assert_eq!(cmd.program, "cat");
        assert_eq!(cmd.stdin_file.as_deref(), Some("input.txt"));
    }

    #[test]
    fn pipe() {
        let cl = parse("ls | grep foo").unwrap();
        let cmds = &cl.first.commands;
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].program, "ls");
        assert_eq!(cmds[1].program, "grep");
        assert_eq!(cmds[1].args, ["foo"]);
    }

    #[test]
    fn multi_pipe() {
        let cl = parse("cat file | grep x | wc -l").unwrap();
        assert_eq!(cl.first.commands.len(), 3);
        assert_eq!(cl.first.commands[2].program, "wc");
    }

    #[test]
    fn and_chain() {
        let cl = parse("a && b").unwrap();
        assert_eq!(cl.first.commands[0].program, "a");
        assert_eq!(cl.rest.len(), 1);
        assert!(matches!(cl.rest[0].0, Connector::And));
        assert_eq!(cl.rest[0].1.commands[0].program, "b");
    }

    #[test]
    fn or_chain() {
        let cl = parse("a || b").unwrap();
        assert!(matches!(cl.rest[0].0, Connector::Or));
    }

    #[test]
    fn semicolon_chain() {
        let cl = parse("a; b; c").unwrap();
        assert_eq!(cl.rest.len(), 2);
        assert!(matches!(cl.rest[0].0, Connector::Seq));
        assert!(matches!(cl.rest[1].0, Connector::Seq));
    }

    #[test]
    fn trailing_semicolon() {
        let cl = parse("a; b;").unwrap();
        assert_eq!(cl.rest.len(), 1);
    }

    #[test]
    fn comment_strips_tail() {
        let cmd = first_cmd("echo hello # world");
        assert_eq!(cmd.args, ["hello"]);
    }

    #[test]
    fn command_substitution() {
        let cmd = first_cmd("echo $(echo inner)");
        assert_eq!(cmd.args, ["inner"]);
    }

    #[test]
    fn backtick_substitution() {
        let cmd = first_cmd("echo `echo back`");
        assert_eq!(cmd.args, ["back"]);
    }

    #[test]
    fn nested_substitution() {
        let cmd = first_cmd("echo $(echo $(echo deep))");
        assert_eq!(cmd.args, ["deep"]);
    }

    #[test]
    fn quoted_glob_not_expanded() {
        let cmd = first_cmd("echo '*.rs'");
        assert_eq!(cmd.args, ["*.rs"]);
    }

    #[test]
    fn empty_input_is_error() {
        assert!(parse("").is_err());
    }

    #[test]
    fn only_comment_is_error() {
        assert!(parse("# just a comment").is_err());
    }

    #[test]
    fn lone_pipe_is_error() {
        assert!(parse("|").is_err());
    }

    #[test]
    fn trailing_pipe_is_error() {
        assert!(parse("ls |").is_err());
    }

    #[test]
    fn lone_ampersand_is_error() {
        assert!(parse("&").is_err());
    }

    #[test]
    fn missing_redirect_file() {
        assert!(parse("echo >").is_err());
        assert!(parse("cat <").is_err());
    }

    #[test]
    fn unclosed_quote() {
        assert!(parse("echo 'hello").is_err());
        assert!(parse("echo \"hello").is_err());
    }

    #[test]
    fn pipe_with_redirect() {
        let cl = parse("cat file | grep x > out.txt").unwrap();
        let cmds = &cl.first.commands;
        assert_eq!(cmds.len(), 2);
        assert!(cmds[0].stdout_file.is_none());
        assert_eq!(cmds[1].stdout_file.as_deref(), Some("out.txt"));
    }

    #[test]
    fn mixed_chain_and_pipe() {
        let cl = parse("a | b && c | d").unwrap();
        assert_eq!(cl.first.commands.len(), 2);
        assert_eq!(cl.rest.len(), 1);
        assert_eq!(cl.rest[0].1.commands.len(), 2);
    }
}
