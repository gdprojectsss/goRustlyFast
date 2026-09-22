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
];

const UNSUPPORTED_TEXT: &[(&str, &str)] = &[
    (
        "shopt -s extglob",
        "extended glob patterns are not supported",
    ),
    ("declare -n", "name references are not supported"),
    ("local -n", "name references are not supported"),
    ("BASH_SOURCE", "BASH_SOURCE is not set"),
    ("FUNCNAME", "FUNCNAME is not set"),
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
        CommandKind::For { body, .. } => visit(body),
        CommandKind::Case { arms, .. } => arms.iter().for_each(|arm| visit(&arm.body)),
        CommandKind::FunctionDefinition { body, .. } => nested(body, visit),
        CommandKind::Simple(_) | CommandKind::Conditional(_) => {}
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

pub fn check_source(source: &str, path_commands: &[String]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let code = line.split(" #").next().unwrap_or(line);
        if code.trim_start().starts_with('#') {
            continue;
        }
        for (pattern, hint) in UNSUPPORTED_TEXT {
            if code.contains(pattern) {
                findings.push(Finding {
                    line: index + 1,
                    error: true,
                    message: format!("{hint} ({pattern})"),
                });
            }
        }
    }

    let list = match parser::parse_from_line(source, &HashMap::new(), 1) {
        Ok(list) => list,
        Err(error) => {
            if let Some(line) = error.line.filter(|line| *line > 1) {
                let prefix: Vec<&str> = source.lines().take(line - 1).collect();
                let earlier = check_source(&(prefix.join("\n") + "\n"), path_commands);
                findings.extend(earlier.into_iter().filter(|finding| finding.line < line));
            }
            findings.push(Finding {
                line: error.line.unwrap_or_else(|| source.lines().count().max(1)),
                error: true,
                message: if error.incomplete {
                    format!("unexpected end of file: {}", error.message)
                } else {
                    error.message
                },
            });
            findings.sort_by_key(|finding| finding.line);
            return findings;
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
    fn functions_defined_later_still_count() {
        assert!(check("main() { helper; }\nhelper() { echo; }\nmain\n").is_empty());
    }
}
