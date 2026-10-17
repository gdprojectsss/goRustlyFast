use std::collections::{BTreeSet, HashMap};

use crate::builtins;
use crate::commands;
use crate::syntax::ast::{Command, CommandKind, List};
use crate::syntax::parser;
use crate::table;

const UNSUPPORTED_COMMANDS: &[(&str, &str)] = &[
    ("let", "use $(( )) or (( )) arithmetic expansion instead"),
    ("coproc", "coprocesses are not supported"),
    ("select", "select menus are not supported"),
    ("enable", "builtins cannot be toggled"),
    ("caller", "call stack inspection is not supported"),
    ("compgen", "use complete -W or -C instead"),
    ("bind", "use bindkey instead"),
    ("disown", "jobs cannot be disowned yet"),
    ("suspend", "suspend is not supported"),
    ("fc", "use history and !-expansion instead"),
    ("hash", "the command cache is managed automatically"),
    (
        "setopt",
        "zsh only, see https://grf.sh/docs/migrating-from-zsh/",
    ),
    (
        "unsetopt",
        "zsh only, see https://grf.sh/docs/migrating-from-zsh/",
    ),
    (
        "autoload",
        "zsh only, see https://grf.sh/docs/migrating-from-zsh/",
    ),
    ("zstyle", "zsh only, completion is configured with complete"),
    ("zmodload", "zsh only"),
    ("compinit", "zsh only, completion is built in"),
    ("compdef", "zsh only, use complete instead"),
    ("zle", "zsh only, use bindkey instead"),
];

const ZSH_FILES: &[(&str, &str)] = &[
    (
        "oh-my-zsh.sh",
        "oh-my-zsh needs zsh, and grfsh has its main features built in",
    ),
    (
        "p10k",
        "powerlevel10k needs zsh, run configure-prompt for a similar prompt",
    ),
];

const CLOSERS: &[&str] = &[
    "fi", "done", "esac", "}", "then", "else", "elif", "do", ";;",
];

const UNSUPPORTED_TEXT: &[(&str, &str)] = &[
    (
        "shopt -s extglob",
        "extended glob patterns are not supported",
    ),
    ("declare -n", "name references are not supported"),
    ("local -n", "name references are not supported"),
    ("trap DEBUG", "DEBUG traps are not supported"),
];

#[derive(Debug, PartialEq)]
pub struct Finding {
    pub line: usize,
    pub error: bool,
    pub message: String,
}

fn functions(list: &List, names: &mut BTreeSet<String>) {
    for item in list {
        let pipelines =
            std::iter::once(&item.first).chain(item.rest.iter().map(|(_, pipeline)| pipeline));
        for pipeline in pipelines {
            for command in &pipeline.commands {
                nested(command, &mut |inner| functions(inner, names));
                if let CommandKind::FunctionDefinition { name, body } = &command.kind {
                    names.insert(name.clone());
                    nested(body, &mut |inner| functions(inner, names));
                }
            }
        }
    }
}

fn nested(command: &Command, visit: &mut impl FnMut(&List)) {
    match &command.kind {
        CommandKind::Group(list) | CommandKind::Subshell(list) => visit(list),
        CommandKind::If {
            branches,
            otherwise,
        } => {
            for (condition, body) in branches {
                visit(condition);
                visit(body);
            }
            if let Some(list) = otherwise {
                visit(list);
            }
        }
        CommandKind::Loop {
            condition, body, ..
        } => {
            visit(condition);
            visit(body);
        }
        CommandKind::For { body, .. } | CommandKind::ArithmeticFor { body, .. } => visit(body),
        CommandKind::Case { arms, .. } => arms.iter().for_each(|arm| visit(&arm.body)),
        CommandKind::FunctionDefinition { body, .. } => nested(body, visit),
        CommandKind::Simple(_) | CommandKind::Conditional(_) | CommandKind::Arithmetic(_) => {}
    }
}

struct Checker<'a> {
    known: &'a BTreeSet<String>,
    findings: Vec<Finding>,
}

impl Checker<'_> {
    fn list(&mut self, list: &List) {
        for item in list {
            let pipelines =
                std::iter::once(&item.first).chain(item.rest.iter().map(|(_, pipeline)| pipeline));
            for pipeline in pipelines {
                for command in &pipeline.commands {
                    self.command(command, item.line);
                }
            }
        }
    }

    fn command(&mut self, command: &Command, line: usize) {
        if let CommandKind::Simple(simple) = &command.kind {
            let Some(name) = simple.words.first().and_then(|word| word.as_literal()) else {
                return;
            };
            if let Some((_, hint)) = UNSUPPORTED_COMMANDS
                .iter()
                .find(|(command, _)| *command == name)
            {
                self.findings.push(Finding {
                    line,
                    error: true,
                    message: format!("'{name}' is not supported: {hint}"),
                });
            } else if !self.known.contains(name) && !name.contains('/') && !name.contains('=') {
                self.findings.push(Finding {
                    line,
                    error: false,
                    message: format!("'{name}' is not a builtin, function or command on PATH"),
                });
            }
            return;
        }
        nested(command, &mut |inner| self.list(inner));
    }
}

fn without_zsh_flags(line: &str) -> Option<String> {
    if !line.contains("${(") {
        return None;
    }
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find("${(") {
        out.push_str(&rest[..start + 2]);
        let after = &rest[start + 3..];
        match after.find(')') {
            Some(end) => {
                out.push('Z');
                rest = &after[end + 1..];
            }
            None => {
                out.push('(');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Some(out)
}

fn text_findings(source: &str, lines: &mut [String]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let code = line.split(" #").next().unwrap_or(line);
        if code.trim_start().starts_with('#') {
            continue;
        }
        let mut found = |message: String| {
            findings.push(Finding {
                line: index + 1,
                error: true,
                message,
            })
        };
        for (pattern, hint) in UNSUPPORTED_TEXT {
            if code.contains(pattern) {
                found(format!("{hint} ({pattern})"));
            }
        }
        for (pattern, hint) in ZSH_FILES {
            if code.contains(pattern) {
                found((*hint).to_string());
            }
        }
        if let Some(replaced) = without_zsh_flags(code) {
            found("zsh parameter flags such as ${(%)...} are not supported".into());
            lines[index] = replaced;
        }
    }
    findings
}

fn first_word(line: &str) -> &str {
    line.split(|character: char| character.is_whitespace() || character == ';')
        .find(|word| !word.is_empty())
        .unwrap_or("")
}

pub fn check_source(source: &str, path_commands: &[String]) -> Vec<Finding> {
    let mut lines: Vec<String> = source.lines().map(String::from).collect();
    let mut findings = text_findings(source, &mut lines);
    let mut skipped = false;
    let list = loop {
        let text = lines.join("\n") + "\n";
        match parser::parse_from_line(&text, &HashMap::new(), 1) {
            Ok(list) => break list,
            Err(error) => {
                let line = error.line.unwrap_or_else(|| lines.len().max(1));
                let leftover = skipped
                    && CLOSERS
                        .contains(&first_word(lines.get(line - 1).map_or("", String::as_str)));
                if !leftover
                    && !findings
                        .iter()
                        .any(|finding| finding.line == line && finding.error)
                {
                    findings.push(Finding {
                        line,
                        error: true,
                        message: if error.incomplete {
                            format!("unexpected end of file: {}", error.message)
                        } else {
                            error.message
                        },
                    });
                }
                if error.incomplete || line == 0 || line > lines.len() || lines[line - 1].is_empty()
                {
                    break Vec::new();
                }
                lines[line - 1].clear();
                skipped = true;
            }
        }
    };

    let mut known: BTreeSet<String> = path_commands.iter().cloned().collect();
    known.extend(builtins::names().iter().map(|name| name.to_string()));
    known.extend(table::names().iter().map(|name| name.to_string()));
    functions(&list, &mut known);
    let mut checker = Checker {
        known: &known,
        findings,
    };
    checker.list(&list);
    let mut findings = checker.findings;
    findings.sort_by_key(|finding| finding.line);
    findings.dedup();
    findings
}

pub fn run(paths: &[String]) -> i32 {
    if paths.is_empty() {
        crate::shell::error("--check: expected a script path");
        return 2;
    }
    let path_commands = commands::on_path(&std::env::var("PATH").unwrap_or_default());
    let mut status = 0;
    for path in paths {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                crate::shell::error(format!("{path}: {}", crate::sys::error_message(&error)));
                status = 2;
                continue;
            }
        };
        let findings = check_source(&source, &path_commands);
        for finding in &findings {
            let level = if finding.error { "error" } else { "warning" };
            println!("{path}:{}: {level}: {}", finding.line, finding.message);
        }
        if findings.iter().any(|finding| finding.error) {
            status = status.max(1);
        }
        if findings.is_empty() {
            println!("{path}: ok");
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(source: &str) -> Vec<(usize, bool, String)> {
        let path = vec!["ls".to_string(), "grep".to_string()];
        check_source(source, &path)
            .into_iter()
            .map(|finding| (finding.line, finding.error, finding.message))
            .collect()
    }

    #[test]
    fn clean_scripts_have_no_findings() {
        assert!(check("greet() { echo hi; }\ngreet\nls | grep x\n").is_empty());
    }

    #[test]
    fn unknown_and_unsupported_commands() {
        let findings = check("echo start\nmystery-tool --flag\nlet x=1\n");
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].0, 2);
        assert!(!findings[0].1);
        assert_eq!(findings[1].0, 3);
        assert!(findings[1].1);
    }

    #[test]
    fn syntax_errors_and_text_patterns() {
        let findings = check("if true; then\n  echo ok\n");
        assert!(findings[0].2.contains("unexpected end of file"));
        let findings = check("shopt -s extglob\n");
        assert!(findings[0].2.contains("extended glob"));
    }

    #[test]
    fn lines_before_a_syntax_error_are_still_checked() {
        let findings = check("mystery-tool\necho ok\nfi\n");
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].0, 1);
        assert_eq!(findings[1].0, 3);
    }

    #[test]
    fn zsh_setups_get_explained_and_checking_continues() {
        let source = "if [[ -r \"${(%):-%n}.zsh\" ]]; then\n  source \"$HOME/.p10k.zsh\"\nfi\nsetopt autocd\necho ${(s/:/)PATH}\nmystery-tool\n";
        let findings = check(source);
        assert!(
            findings
                .iter()
                .any(|finding| finding.0 == 1 && finding.2.contains("zsh parameter flags"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.0 == 2 && finding.2.contains("powerlevel10k"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.0 == 4 && finding.2.contains("zsh only"))
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.0 == 6 && finding.2.contains("mystery-tool"))
        );
    }

    #[test]
    fn checking_continues_after_a_syntax_error() {
        let findings = check("echo ok\nif then\n  echo inside\nfi\nmystery-tool\n");
        assert!(findings.iter().any(|finding| finding.0 == 2 && finding.1));
        assert!(!findings.iter().any(|finding| finding.0 == 4));
        assert!(
            findings
                .iter()
                .any(|finding| finding.0 == 5 && finding.2.contains("mystery-tool"))
        );
    }

    #[test]
    fn functions_defined_later_still_count() {
        assert!(check("main() { helper; }\nhelper() { echo; }\nmain\n").is_empty());
    }
}
