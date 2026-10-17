use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use crate::builtins;
use crate::expand;
use crate::shell::Shell;
use crate::syntax::ast::{
    AssignedValue, Command, CommandKind, List, Redirect, RedirectKind, SimpleCommand, Word,
    WordPart,
};
use crate::syntax::parser;
use crate::table;

const STATE_BUILTINS: &[&str] = &[
    "cd", "pushd", "popd", "export", "declare", "typeset", "local", "readonly", "set", "shopt",
    "unset", "alias", "unalias", "shift",
];
const PREFIXES: &[&str] = &[
    "sudo", "doas", "env", "command", "nice", "nohup", "time", "exec",
];

fn quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./=:@%+,~".contains(character))
    {
        return text.to_string();
    }
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn source_of(word: &Word) -> String {
    word.parts
        .iter()
        .map(|part| match part {
            WordPart::Literal(text) => text.clone(),
            WordPart::Quoted(text) => format!("\"{text}\""),
            WordPart::Tilde(user) => format!("~{user}"),
            WordPart::Parameter { name, .. } | WordPart::Indirect { name, .. } => {
                format!("${{{name}}}")
            }
            WordPart::Length { name, .. } => format!("${{#{name}}}"),
            WordPart::Indices { name, .. } => format!("${{!{name}[@]}}"),
            WordPart::Names { prefix, .. } => format!("${{!{prefix}*}}"),
            WordPart::CommandSubstitution { source, .. } => format!("$({source})"),
            WordPart::ProcessSubstitution { source, output } => {
                format!("{}({source})", if *output { '>' } else { '<' })
            }
            WordPart::Arithmetic { expression, .. } => format!("$(({}))", source_of(expression)),
        })
        .collect()
}

fn has_glob(word: &Word) -> bool {
    word.parts
        .iter()
        .any(|part| matches!(part, WordPart::Literal(text) if text.contains(['*', '?', '['])))
}

fn parameters(word: &Word) -> Vec<&str> {
    word.parts
        .iter()
        .filter_map(|part| match part {
            WordPart::Parameter {
                name,
                operator: None,
                ..
            } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

fn flag(args: &[String], short: char, long: &str) -> bool {
    args.iter().any(|argument| {
        argument == long
            || (argument.starts_with('-')
                && !argument.starts_with("--")
                && argument[1..].contains(short))
    })
}

fn operands(args: &[String]) -> Vec<&String> {
    args.iter()
        .filter(|argument| !argument.starts_with('-'))
        .collect()
}

fn risky_path(path: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let trimmed = path.trim_end_matches('/');
    matches!(trimmed, "" | "." | ".." | "~" | "*" | "/*" | "./*")
        || trimmed == home
        || path == "/"
        || (path.starts_with('/') && path.matches('/').count() == 1 && !path.contains('.'))
}

fn destructive(argv: &[String]) -> Vec<String> {
    let mut start = 0;
    while argv
        .get(start)
        .is_some_and(|word| PREFIXES.contains(&word.as_str()) || word.contains('='))
    {
        start += 1;
    }
    let Some(name) = argv.get(start) else {
        return Vec::new();
    };
    let args = &argv[start + 1..];
    let mut notes = Vec::new();
    let has = |word: &str| args.iter().any(|argument| argument == word);
    match name.rsplit('/').next().unwrap_or(name) {
        "rm" => {
            let targets = operands(args);
            let how = if flag(args, 'r', "--recursive") || flag(args, 'R', "--recursive") {
                "deletes recursively"
            } else {
                "deletes"
            };
            let shown: Vec<String> = targets.iter().take(10).map(|path| quote(path)).collect();
            let more = match targets.len().saturating_sub(10) {
                0 => String::new(),
                count => format!(" and {count} more"),
            };
            notes.push(format!("{how}: {}{more}", shown.join(" ")));
            let risky: Vec<&&String> = targets.iter().filter(|path| risky_path(path)).collect();
            match risky.as_slice() {
                [] => {}
                [path] => notes.push(format!("warning: this deletes {path}")),
                paths => notes.push(format!(
                    "warning: this deletes {} top level paths, starting with {}",
                    paths.len(),
                    paths[0]
                )),
            }
        }
        "rmdir" | "unlink" | "shred" | "truncate" => {
            notes.push(format!("{name} changes or removes files"))
        }
        "dd" if args.iter().any(|argument| argument.starts_with("of=")) => {
            notes.push("dd writes directly to its of= target".into());
        }
        command if command.starts_with("mkfs") || command == "fdisk" || command == "wipefs" => {
            notes.push("warning: formats or repartitions a disk".into());
        }
        "chmod" | "chown" | "chgrp" if flag(args, 'R', "--recursive") => {
            notes.push(format!("{name} applies recursively"));
        }
        "find"
            if has("-delete")
                || args
                    .windows(2)
                    .any(|pair| pair[0] == "-exec" && pair[1] == "rm") =>
        {
            notes.push("find deletes what it matches".into());
        }
        "mv" | "cp" => {
            if let Some(destination) = operands(args).last()
                && Path::new(destination.as_str()).is_file()
                && operands(args).len() == 2
            {
                notes.push(format!("overwrites the existing file {destination}"));
            }
        }
        "kill" | "killall" | "pkill" => notes.push("sends signals to processes".into()),
        "git" => {
            let sub = args.first().map(String::as_str).unwrap_or_default();
            let rest = &args[args.len().min(1)..];
            let note = match sub {
                "reset" if rest.iter().any(|word| word == "--hard") => {
                    Some("git reset --hard discards uncommitted changes")
                }
                "clean" if flag(rest, 'f', "--force") => Some("git clean deletes untracked files"),
                "push"
                    if flag(rest, 'f', "--force")
                        || rest
                            .iter()
                            .any(|word| word.starts_with("--force") || word.starts_with('+')) =>
                {
                    Some("force push rewrites the remote branch")
                }
                "checkout" | "restore" if rest.iter().any(|word| word == "." || word == "--") => {
                    Some("discards changes in the working tree")
                }
                "branch" if rest.iter().any(|word| word == "-D") => {
                    Some("deletes a branch even if it is not merged")
                }
                "stash"
                    if rest
                        .first()
                        .is_some_and(|word| word == "drop" || word == "clear") =>
                {
                    Some("deletes stashed changes")
                }
                _ => None,
            };
            notes.extend(note.map(String::from));
        }
        "docker" | "podman"
            if args
                .first()
                .is_some_and(|word| matches!(word.as_str(), "rm" | "rmi" | "prune"))
                || (has("prune") && !has("--help")) =>
        {
            notes.push(format!("{name} removes containers, images or volumes"));
        }
        "kubectl" if args.first().is_some_and(|word| word == "delete") => {
            notes.push("kubectl deletes cluster resources".into());
        }
        _ => {}
    }
    notes
}

struct Explainer<'a> {
    shell: &'a mut Shell,
    lines: Vec<String>,
    functions: HashMap<String, usize>,
}

impl Explainer<'_> {
    fn emit(&mut self, line: usize, text: String) {
        self.lines.push(format!("{line:>4}  {text}"));
    }

    fn note(&mut self, text: String) {
        self.lines.push(format!("        {text}"));
    }

    fn list(&mut self, list: &List) {
        for item in list {
            let pipelines =
                std::iter::once(&item.first).chain(item.rest.iter().map(|(_, pipeline)| pipeline));
            for pipeline in pipelines {
                for command in &pipeline.commands {
                    self.command(command, item.line);
                }
            }
            if item.background {
                self.note("runs in the background".into());
            }
        }
    }

    fn command(&mut self, command: &Command, line: usize) {
        self.shell.current_line = line;
        match &command.kind {
            CommandKind::Simple(simple) => self.simple(simple, command, line),
            CommandKind::Group(list) | CommandKind::Subshell(list) => self.list(list),
            CommandKind::If {
                branches,
                otherwise,
            } => {
                for (condition, body) in branches {
                    self.list(condition);
                    self.list(body);
                }
                if let Some(list) = otherwise {
                    self.list(list);
                }
            }
            CommandKind::Loop {
                condition, body, ..
            } => {
                self.list(condition);
                self.list(body);
            }
            CommandKind::For {
                variable,
                items,
                body,
            } => {
                let values = match items {
                    Some(words) => expand::words(self.shell, words),
                    None => Ok(self.shell.positional.clone()),
                };
                match values {
                    Ok(values) => {
                        let shown: Vec<String> =
                            values.iter().take(20).map(|value| quote(value)).collect();
                        let more = values.len().saturating_sub(20);
                        let suffix = if more > 0 {
                            format!(" and {more} more")
                        } else {
                            String::new()
                        };
                        self.emit(
                            line,
                            format!("for {variable} in {}{suffix}", shown.join(" ")),
                        );
                        if let Some(first) = values.first() {
                            self.shell.set_variable(variable, first);
                            self.note(format!(
                                "the body below is shown with {variable}={}",
                                quote(first)
                            ));
                        }
                    }
                    Err(error) => self.emit(line, format!("for {variable}: {error}")),
                }
                self.list(body);
            }
            CommandKind::Case { subject, arms } => {
                match expand::words(self.shell, std::slice::from_ref(subject)) {
                    Ok(value) => self.emit(line, format!("case {}", quote(&value.join(" ")))),
                    Err(error) => self.emit(line, format!("case: {error}")),
                }
                for arm in arms {
                    self.list(&arm.body);
                }
            }
            CommandKind::Conditional(_) => self.emit(line, "[[ condition ]]".into()),
            CommandKind::Arithmetic(word) => {
                self.emit(line, format!("(( {} ))", source_of(word)));
            }
            CommandKind::ArithmeticFor { body, .. } => {
                self.emit(line, "for (( ... )) loop".into());
                self.list(body);
            }
            CommandKind::FunctionDefinition { name, .. } => {
                self.emit(line, format!("defines function {name}"));
                self.functions.insert(name.clone(), line);
                self.shell.run_command(command, false);
            }
        }
        if !matches!(command.kind, CommandKind::Simple(_)) {
            self.redirects(&command.redirects);
        }
    }

    fn redirects(&mut self, redirects: &[Redirect]) {
        for redirect in redirects {
            let symbol = match redirect.kind {
                RedirectKind::In => "<",
                RedirectKind::Out => ">",
                RedirectKind::Clobber => ">|",
                RedirectKind::Append => ">>",
                RedirectKind::ReadWrite => "<>",
                RedirectKind::DuplicateIn => "<&",
                RedirectKind::DuplicateOut => ">&",
                RedirectKind::OutAndError => "&>",
                RedirectKind::AppendAndError => "&>>",
                RedirectKind::Heredoc => {
                    self.note("reads a here document".into());
                    continue;
                }
                RedirectKind::HereString => "<<<",
            };
            let target = expand::words(self.shell, std::slice::from_ref(&redirect.target))
                .map(|words| words.join(" "))
                .unwrap_or_else(|error| error);
            let mut text = format!(
                "redirect {}{symbol} {}",
                if redirect.fd > 2 {
                    redirect.fd.to_string()
                } else {
                    String::new()
                },
                quote(&target)
            );
            if matches!(
                redirect.kind,
                RedirectKind::Out | RedirectKind::OutAndError | RedirectKind::Clobber
            ) && !target.starts_with("/dev/")
                && Path::new(&target).is_file()
            {
                text.push_str(", overwriting the existing file");
            }
            self.note(text);
            self.empty_parameters(&redirect.target);
        }
    }

    fn empty_parameters(&mut self, word: &Word) {
        for name in parameters(word) {
            if self
                .shell
                .get_variable(name)
                .is_none_or(|value| value.is_empty())
            {
                self.note(format!("warning: ${name} is empty or unset here"));
            }
        }
    }

    fn simple(&mut self, simple: &SimpleCommand, command: &Command, line: usize) {
        if simple.words.is_empty() {
            let shown: Vec<String> = simple
                .assignments
                .iter()
                .map(|assignment| match &assignment.value {
                    AssignedValue::Scalar(word) => {
                        format!("{}={}", assignment.name, source_of(word))
                    }
                    AssignedValue::Array(_) => format!("{}=(...)", assignment.name),
                })
                .collect();
            self.shell.run_simple(simple, &[], false);
            let values: Vec<String> = simple
                .assignments
                .iter()
                .map(|assignment| {
                    let value = self
                        .shell
                        .get_variable(&assignment.name)
                        .unwrap_or_default();
                    format!("{}={}", assignment.name, quote(&value))
                })
                .collect();
            self.emit(line, format!("sets {}", values.join(" ")));
            if shown.iter().any(|text| text.contains(['$', '`'])) {
                self.note(format!("from {}", shown.join(" ")));
            }
            self.redirects(&command.redirects);
            return;
        }
        let argv = match expand::words(self.shell, &simple.words) {
            Ok(argv) => argv,
            Err(error) => {
                self.emit(line, format!("expansion fails: {error}"));
                return;
            }
        };
        let Some(name) = argv.first().cloned() else {
            self.emit(line, "expands to nothing, so no command runs".into());
            return;
        };
        let prefix: String = simple
            .assignments
            .iter()
            .map(|assignment| format!("{}=... ", assignment.name))
            .collect();
        let shown: Vec<String> = argv.iter().map(|word| quote(word)).collect();
        self.emit(line, format!("{prefix}{}", shown.join(" ")));

        for word in &simple.words {
            if has_glob(word) {
                let matched =
                    expand::words(self.shell, std::slice::from_ref(word)).unwrap_or_default();
                let pattern = source_of(word);
                let literal = matched.len() == 1 && matched[0] == expand::glob::unescape(&pattern);
                if literal || matched.is_empty() {
                    self.note(format!("pattern {pattern} matches nothing"));
                } else {
                    self.note(format!("pattern {pattern} matches {} paths", matched.len()));
                }
            }
            if word
                .parts
                .iter()
                .any(|part| matches!(part, WordPart::CommandSubstitution { .. }))
            {
                self.note(format!("{} is not run while explaining", source_of(word)));
            }
        }

        let notes = destructive(&argv);
        if !notes.is_empty() {
            for word in &simple.words[1..] {
                self.empty_parameters(word);
            }
        }
        for note in notes {
            self.note(note);
        }

        if let Some(defined) = self.functions.get(&name) {
            self.note(format!("calls the function defined on line {defined}"));
        } else if self.shell.functions.contains_key(&name) {
            self.note("calls a shell function".into());
        } else if builtins::is_builtin(&name) {
            if STATE_BUILTINS.contains(&name.as_str()) {
                self.shell.run_simple(simple, &[], false);
            }
        } else if !table::is_command(&name) {
            match self.shell.resolve(&name) {
                Some(path) if path != name => self.note(format!("runs {path}")),
                Some(_) => {}
                None => self.note(format!(
                    "warning: {name} is not a command, function or builtin"
                )),
            }
        }
        self.redirects(&command.redirects);
    }
}

pub(crate) fn explain(shell: &mut Shell, source: &str, out: &mut dyn Write) -> i32 {
    let list = match parser::parse_from_line(source, &shell.aliases, 1) {
        Ok(list) => list,
        Err(error) => {
            let _ = writeln!(out, "syntax error: {error}");
            return 2;
        }
    };
    shell.explaining = true;
    let mut explainer = Explainer {
        shell,
        lines: Vec::new(),
        functions: HashMap::new(),
    };
    explainer.list(&list);
    for line in &explainer.lines {
        let _ = writeln!(out, "{line}");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(command: &str) -> Vec<String> {
        destructive(
            &command
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn destructive_commands_are_flagged() {
        assert_eq!(notes("rm -rf build"), ["deletes recursively: build"]);
        assert!(
            notes("sudo rm -rf /")
                .iter()
                .any(|note| note.contains("warning"))
        );
        assert!(notes("git reset --hard HEAD")[0].contains("discards"));
        assert!(notes("git push -f origin main")[0].contains("force push"));
        assert!(notes("git push origin main").is_empty());
        assert!(notes("ls -la").is_empty());
    }
}
