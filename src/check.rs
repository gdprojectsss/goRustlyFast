use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::builtins;
use crate::commands;
use crate::syntax::ast::{
    AndOr, AssignedValue, Command, CommandKind, List, ParameterOperation, Word, WordPart,
};
use crate::syntax::parser::{self, ParseError};
use crate::table;

enum Trigger {
    Command(&'static str),
    Flag(&'static str, char),
    Argument(&'static str, &'static str),
    Variable(&'static str),
}

struct Unsupported {
    trigger: Trigger,
    message: &'static str,
}

const fn entry(trigger: Trigger, message: &'static str) -> Unsupported {
    Unsupported { trigger, message }
}

const UNSUPPORTED: &[Unsupported] = &[
    entry(Trigger::Command("bind"), "use bindkey instead"),
    entry(Trigger::Command("suspend"), "suspend is not supported"),
    entry(
        Trigger::Flag("declare", 'n'),
        "name references are not supported",
    ),
    entry(
        Trigger::Flag("typeset", 'n'),
        "name references are not supported",
    ),
    entry(
        Trigger::Flag("local", 'n'),
        "name references are not supported",
    ),
    entry(
        Trigger::Flag("set", 'a'),
        "set -a (allexport) is not supported",
    ),
    entry(
        Trigger::Argument("set", "allexport"),
        "set -o allexport is not supported",
    ),
    entry(
        Trigger::Variable("BASH_VERSINFO"),
        "grfsh does not set BASH_VERSINFO, since it is not bash",
    ),
    entry(
        Trigger::Variable("BASH_LINENO"),
        "BASH_LINENO is not set in grfsh",
    ),
];

const ZSH_COMMANDS: &[(&str, &str)] = &[
    ("setopt", "see https://grf.sh/docs/migrating-from-zsh/"),
    ("unsetopt", "see https://grf.sh/docs/migrating-from-zsh/"),
    ("autoload", "see https://grf.sh/docs/migrating-from-zsh/"),
    ("zstyle", "completion is configured with complete"),
    ("zmodload", "grfsh has no modules"),
    ("compinit", "completion is built in"),
    ("compdef", "use complete instead"),
    ("zle", "use bindkey instead"),
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

const GUARDS: &[&str] = &["command", "type", "hash", "which"];
const SOURCE_DEPTH: usize = 8;

#[derive(Debug, PartialEq)]
pub struct Finding {
    pub line: usize,
    pub error: bool,
    pub message: String,
}

fn error(line: usize, message: String) -> Finding {
    Finding {
        line,
        error: true,
        message,
    }
}

fn warning(line: usize, message: String) -> Finding {
    Finding {
        line,
        error: false,
        message,
    }
}

fn word_text(word: &Word) -> Option<String> {
    word.parts
        .iter()
        .map(|part| match part {
            WordPart::Literal(text) | WordPart::Quoted(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn has_flag(argument: &str, letter: char) -> bool {
    argument.len() > 1
        && argument.starts_with('-')
        && !argument.starts_with("--")
        && argument[1..].contains(letter)
}

fn command_matches(trigger: &Trigger, name: &str, args: &[String]) -> bool {
    match *trigger {
        Trigger::Command(command) => command == name && !builtins::is_builtin(name),
        Trigger::Flag(command, letter) => {
            command == name && args.iter().any(|argument| has_flag(argument, letter))
        }
        Trigger::Argument(command, expected) => {
            command == name && args.iter().any(|argument| argument == expected)
        }
        _ => false,
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

fn code_of(line: &str) -> Option<&str> {
    let code = line.split(" #").next().unwrap_or(line);
    (!code.trim_start().starts_with('#')).then_some(code)
}

fn text_findings(lines: &mut [String]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, line) in lines.iter_mut().enumerate() {
        let Some(code) = code_of(line) else {
            continue;
        };
        let number = index + 1;
        for unsupported in UNSUPPORTED {
            let Trigger::Variable(pattern) = unsupported.trigger else {
                continue;
            };
            if code.contains(pattern) {
                let message = if unsupported.message.contains(pattern) {
                    unsupported.message.to_string()
                } else {
                    format!("{} ({pattern})", unsupported.message)
                };
                findings.push(warning(number, message));
            }
        }
        for (pattern, hint) in ZSH_FILES {
            if code.contains(pattern) {
                findings.push(error(number, (*hint).to_string()));
            }
        }
        if let Some(replaced) = without_zsh_flags(code) {
            findings.push(error(
                number,
                "zsh parameter flags such as ${(%)...} are not supported".into(),
            ));
            *line = replaced;
        }
    }
    findings
}

fn first_word(line: &str) -> &str {
    line.split(|character: char| character.is_whitespace() || character == ';')
        .find(|word| !word.is_empty())
        .unwrap_or("")
}

fn unclosed_quote_line(text: &str) -> Option<usize> {
    let mut line = 0;
    let mut open: Option<(char, usize)> = None;
    let mut chars = text.chars().peekable();
    let mut word_start = true;
    while let Some(character) = chars.next() {
        if character == '\n' {
            line += 1;
        }
        match (open, character) {
            (Some(('\'', _)), '\'') | (Some(('"', _)), '"') => open = None,
            (Some(('"', _)), '\\') => {
                if chars.next() == Some('\n') {
                    line += 1;
                }
            }
            (Some(_), _) => {}
            (None, '\\') => {
                if chars.next() == Some('\n') {
                    line += 1;
                }
            }
            (None, '#') if word_start => {
                while chars.peek().is_some_and(|next| *next != '\n') {
                    chars.next();
                }
            }
            (None, '\'' | '"') => open = Some((character, line)),
            _ => {}
        }
        word_start = character.is_whitespace() || ";|&(".contains(character);
    }
    open.map(|(_, line)| line)
}

fn ends_script(item: &AndOr) -> bool {
    if !item.rest.is_empty()
        || item.background
        || item.first.negate
        || item.first.commands.len() != 1
    {
        return false;
    }
    let CommandKind::Simple(simple) = &item.first.commands[0].kind else {
        return false;
    };
    match simple.words.first().and_then(Word::as_literal) {
        Some("exit") => true,
        Some("exec") => simple.words.len() > 1,
        _ => false,
    }
}

struct Script {
    list: List,
    last_line: usize,
}

fn describe(error: &ParseError) -> String {
    if error.incomplete {
        format!("unexpected end of file: {}", error.message)
    } else {
        error.message.clone()
    }
}

fn report_syntax_error(
    findings: &mut Vec<Finding>,
    error: &ParseError,
    line: usize,
    lines: &[String],
    after_error: bool,
) {
    let line_text = lines.get(line.saturating_sub(1)).map_or("", String::as_str);
    if after_error && CLOSERS.contains(&first_word(line_text)) {
        return;
    }
    if findings
        .iter()
        .any(|finding| finding.line == line && finding.error)
    {
        return;
    }
    findings.push(self::error(line, describe(error)));
}

fn joined(lines: &[String]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn until_exit(mut list: List, total_lines: usize) -> Script {
    match list.iter().position(ends_script) {
        Some(position) => {
            let last_line = list[position].line;
            list.truncate(position + 1);
            Script { list, last_line }
        }
        None => Script {
            list,
            last_line: total_lines,
        },
    }
}

fn exit_before(lines: &[String], error_line: usize) -> Option<Script> {
    let end = error_line.saturating_sub(1).min(lines.len());
    for (index, line) in lines[..end].iter().enumerate() {
        if !matches!(first_word(line), "exit" | "exec") {
            continue;
        }
        let Ok(list) = parser::parse_with_extglob(&joined(&lines[..=index])) else {
            continue;
        };
        let script = until_exit(list, lines.len());
        if script.last_line <= index + 1 {
            return Some(script);
        }
    }
    None
}

fn parse_script(lines: &[String], findings: &mut Vec<Finding>) -> Script {
    let mut lines = lines.to_vec();
    let mut after_error = false;
    loop {
        let text = joined(&lines);
        let problem = match parser::parse_with_extglob(&text) {
            Ok(list) => return until_exit(list, lines.len()),
            Err(problem) => problem,
        };
        let error_line = problem.line.unwrap_or(lines.len().max(1));
        if let Some(script) = exit_before(&lines, error_line) {
            return script;
        }
        let line = match unclosed_quote_line(&text) {
            Some(offset) if problem.incomplete && problem.message.contains("quote") => offset + 1,
            _ => error_line,
        };
        report_syntax_error(findings, &problem, line, &lines, after_error);
        after_error = true;
        if problem.incomplete
            || error_line == 0
            || error_line > lines.len()
            || lines[error_line - 1].is_empty()
        {
            return Script {
                list: List::new(),
                last_line: lines.len(),
            };
        }
        lines[error_line - 1].clear();
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
        CommandKind::For { body, .. }
        | CommandKind::ArithmeticFor { body, .. }
        | CommandKind::Select { body, .. } => visit(body),
        CommandKind::Coproc { body, .. } => nested(body, visit),
        CommandKind::Case { arms, .. } => arms.iter().for_each(|arm| visit(&arm.body)),
        CommandKind::FunctionDefinition { body, .. } => nested(body, visit),
        CommandKind::Simple(_) | CommandKind::Conditional(_) | CommandKind::Arithmetic(_) => {}
    }
}

fn commands_of(item: &AndOr) -> impl Iterator<Item = &Command> {
    std::iter::once(&item.first)
        .chain(item.rest.iter().map(|(_, pipeline)| pipeline))
        .flat_map(|pipeline| pipeline.commands.iter())
}

fn simple_name(command: &Command) -> Option<(&str, Vec<String>)> {
    let CommandKind::Simple(simple) = &command.kind else {
        return None;
    };
    let name = simple.words.first()?.as_literal()?;
    let args = simple.words[1..].iter().filter_map(word_text).collect();
    Some((name, args))
}

fn defined_functions(list: &List, names: &mut BTreeSet<String>) {
    for item in list {
        for command in commands_of(item) {
            if let CommandKind::FunctionDefinition { name, .. } = &command.kind {
                names.insert(name.clone());
            }
            nested(command, &mut |inner| defined_functions(inner, names));
        }
    }
}

fn is_script_directory_command(source: &str) -> bool {
    source.contains("dirname")
        && (source.contains("$0") || source.contains("${0") || source.contains("BASH_SOURCE"))
}

#[derive(Default)]
struct Sources {
    script_directory: Option<PathBuf>,
    directory_variables: BTreeSet<String>,
    unresolved: bool,
    functions: BTreeSet<String>,
    visited: BTreeSet<PathBuf>,
}

impl Sources {
    fn word_path(&self, word: &Word) -> Option<String> {
        let directory = self
            .script_directory
            .as_ref()
            .map(|path| path.display().to_string());
        let mut out = String::new();
        for part in &word.parts {
            match part {
                WordPart::Literal(text) | WordPart::Quoted(text) => out.push_str(text),
                WordPart::Tilde(user) if user.is_empty() => {
                    out.push_str(&crate::bytes::env_var("HOME")?);
                }
                WordPart::Parameter {
                    name,
                    operator: None,
                    ..
                } if self.directory_variables.contains(name) => out.push_str(directory.as_deref()?),
                WordPart::Parameter {
                    name,
                    operator: Some(operation),
                    ..
                } if (name == "0" || name == "BASH_SOURCE")
                    && matches!(**operation, ParameterOperation::RemoveSuffix { .. }) =>
                {
                    out.push_str(directory.as_deref()?);
                }
                WordPart::CommandSubstitution { source, .. }
                    if is_script_directory_command(source) =>
                {
                    out.push_str(directory.as_deref()?);
                }
                _ => return None,
            }
        }
        Some(out)
    }

    fn locate(&self, path: &str) -> Option<PathBuf> {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            return candidate.is_file().then(|| candidate.to_path_buf());
        }
        let mut places: Vec<PathBuf> = Vec::new();
        if !path.contains('/') {
            let search = crate::bytes::env_var("PATH").unwrap_or_default();
            places.extend(
                search
                    .split(':')
                    .map(|directory| Path::new(directory).join(path)),
            );
        }
        if let Some(directory) = &self.script_directory {
            places.push(directory.join(path));
        }
        places.push(candidate.to_path_buf());
        places.into_iter().find(|place| place.is_file())
    }

    fn resolve(&self, word: &Word) -> Option<PathBuf> {
        self.locate(&self.word_path(word)?)
    }

    fn follow(&mut self, path: &Path, depth: usize) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if depth > SOURCE_DEPTH || !self.visited.insert(canonical) {
            return names;
        }
        let Ok(bytes) = std::fs::read(path) else {
            self.unresolved = true;
            return names;
        };
        let Ok(list) = parser::parse(&String::from_utf8_lossy(&bytes)) else {
            self.unresolved = true;
            return names;
        };
        defined_functions(&list, &mut names);
        let saved = self.script_directory.clone();
        self.script_directory = path.parent().map(Path::to_path_buf);
        self.scan(&list, depth + 1, &mut names);
        self.script_directory = saved;
        names
    }

    fn scan(&mut self, list: &List, depth: usize, names: &mut BTreeSet<String>) {
        for item in list {
            for command in commands_of(item) {
                self.scan_command(command, depth, names);
                nested(command, &mut |inner| self.scan(inner, depth, names));
            }
        }
    }

    fn scan_command(&mut self, command: &Command, depth: usize, names: &mut BTreeSet<String>) {
        let CommandKind::Simple(simple) = &command.kind else {
            return;
        };
        for assignment in &simple.assignments {
            if let AssignedValue::Scalar(word) = &assignment.value
                && self
                    .word_path(word)
                    .is_some_and(|path| Some(Path::new(&path)) == self.script_directory.as_deref())
            {
                self.directory_variables.insert(assignment.name.clone());
            }
        }
        let is_source = simple
            .words
            .first()
            .and_then(Word::as_literal)
            .is_some_and(|name| name == "source" || name == ".");
        if !is_source {
            return;
        }
        match simple.words.get(1).and_then(|word| self.resolve(word)) {
            Some(path) => {
                let found = self.follow(&path, depth);
                names.extend(found);
            }
            None => self.unresolved = true,
        }
    }
}

#[derive(Clone, Default)]
struct Scope {
    in_function: bool,
    zsh_only: bool,
    guarded: Vec<String>,
}

struct Checker<'a> {
    known: &'a BTreeSet<String>,
    functions: &'a BTreeSet<String>,
    top_definitions: HashMap<String, usize>,
    nested_definitions: BTreeSet<String>,
    defined: BTreeSet<String>,
    sources: Sources,
    findings: Vec<Finding>,
}

fn condition_text(list: &List) -> String {
    list.iter()
        .map(|item| item.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn guard_arguments<'a>(commands: impl Iterator<Item = &'a Command>) -> Vec<String> {
    let mut names = Vec::new();
    for command in commands {
        if let Some((name, args)) = simple_name(command)
            && GUARDS.contains(&name)
        {
            names.extend(
                args.into_iter()
                    .filter(|argument| !argument.starts_with('-')),
            );
        }
    }
    names
}

fn guarded_names(list: &List) -> Vec<String> {
    guard_arguments(list.iter().flat_map(commands_of))
}

fn mentions_zsh(text: &str) -> bool {
    text.contains("ZSH_VERSION") || text.contains("ZSH_NAME") || text.contains("zsh")
}

fn record_definitions(
    list: &List,
    in_function: bool,
    top: &mut HashMap<String, usize>,
    inner: &mut BTreeSet<String>,
) {
    for item in list {
        for command in commands_of(item) {
            if let CommandKind::FunctionDefinition { name, body, .. } = &command.kind {
                if in_function {
                    inner.insert(name.clone());
                } else {
                    top.entry(name.clone()).or_insert(item.line);
                }
                nested(body, &mut |list| record_definitions(list, true, top, inner));
                continue;
            }
            nested(command, &mut |list| {
                record_definitions(list, in_function, top, inner)
            });
        }
    }
}

impl Checker<'_> {
    fn list(&mut self, list: &List, scope: &Scope) {
        for item in list {
            let mut scope = scope.clone();
            if mentions_zsh(&item.text) && !item.rest.is_empty() {
                scope.zsh_only = true;
            }
            let guards = guard_arguments(item.first.commands.iter());
            for (index, command) in commands_of(item).enumerate() {
                if index > 0 {
                    scope.guarded.extend(guards.iter().cloned());
                }
                self.command(command, item.line, &scope);
            }
        }
    }

    fn command(&mut self, command: &Command, line: usize, scope: &Scope) {
        match &command.kind {
            CommandKind::Simple(_) => self.simple(command, line, scope),
            CommandKind::FunctionDefinition { name, body, .. } => {
                if !scope.in_function {
                    self.defined.insert(name.clone());
                }
                let inner = Scope {
                    in_function: true,
                    ..scope.clone()
                };
                self.command(body, line, &inner);
            }
            CommandKind::If {
                branches,
                otherwise,
            } => {
                let mut guarded = scope.clone();
                let mut fallback = scope.clone();
                for (condition, _) in branches {
                    let text = condition_text(condition);
                    guarded.zsh_only |= mentions_zsh(&text);
                    fallback.zsh_only |= text.contains("BASH_VERSION");
                    guarded.guarded.extend(guarded_names(condition));
                }
                for (condition, body) in branches {
                    self.list(condition, scope);
                    self.list(body, &guarded);
                }
                if let Some(list) = otherwise {
                    self.list(list, &fallback);
                }
            }
            _ => nested(command, &mut |inner| self.list(inner, scope)),
        }
    }

    fn simple(&mut self, command: &Command, line: usize, scope: &Scope) {
        let CommandKind::Simple(simple) = &command.kind else {
            return;
        };
        let Some((name, args)) = simple_name(command) else {
            return;
        };
        if let Some(unsupported) = UNSUPPORTED
            .iter()
            .find(|unsupported| command_matches(&unsupported.trigger, name, &args))
            .filter(|_| !self.functions.contains(name))
        {
            let message = match unsupported.trigger {
                Trigger::Command(_) => {
                    format!("'{name}' is not supported: {}", unsupported.message)
                }
                _ => unsupported.message.to_string(),
            };
            self.findings.push(error(line, message));
            return;
        }
        if name == "source" || name == "." {
            self.source(simple.words.get(1), scope);
        }
        if let Some((_, hint)) = ZSH_COMMANDS.iter().find(|(command, _)| *command == name) {
            if !scope.zsh_only {
                self.findings.push(warning(
                    line,
                    format!("'{name}' only exists in zsh, {hint}"),
                ));
            }
            return;
        }
        if self.called_too_early(name, scope) {
            let defined = self.top_definitions.get(name).copied().unwrap_or_default();
            self.findings.push(error(
                line,
                format!("'{name}' is called before it is defined on line {defined}"),
            ));
            return;
        }
        let unknown = !self.known.contains(name)
            && !self.functions.contains(name)
            && !self.sources.functions.contains(name)
            && !self.sources.unresolved
            && !scope.zsh_only
            && !scope.guarded.iter().any(|guard| guard == name)
            && !name.contains('/')
            && !name.contains('=');
        if unknown {
            self.findings.push(warning(
                line,
                format!("'{name}' is not a builtin, function or command on PATH"),
            ));
        }
    }

    fn source(&mut self, word: Option<&Word>, scope: &Scope) {
        if scope.in_function {
            return;
        }
        if let Some(path) = word.and_then(|word| self.sources.resolve(word)) {
            let mut names = BTreeSet::new();
            let saved = std::mem::take(&mut self.sources.visited);
            names.extend(self.sources.follow(&path, 0));
            self.sources.visited = saved;
            self.defined.extend(names);
        }
    }

    fn called_too_early(&self, name: &str, scope: &Scope) -> bool {
        !scope.in_function
            && self.top_definitions.contains_key(name)
            && !self.defined.contains(name)
            && !self.nested_definitions.contains(name)
            && !self.known.contains(name)
            && !self.sources.unresolved
    }
}

fn known_names(path_commands: &[String], list: &List) -> BTreeSet<String> {
    let mut known: BTreeSet<String> = path_commands.iter().cloned().collect();
    known.extend(builtins::names().iter().map(|name| name.to_string()));
    known.extend(table::names().iter().map(|name| name.to_string()));
    aliases(list, &mut known);
    known
}

fn aliases(list: &List, names: &mut BTreeSet<String>) {
    for item in list {
        for command in commands_of(item) {
            if let Some(("alias", args)) = simple_name(command) {
                names.extend(
                    args.iter()
                        .filter_map(|argument| argument.split_once('='))
                        .map(|(name, _)| name.to_string()),
                );
            }
            nested(command, &mut |inner| aliases(inner, names));
        }
    }
}

pub fn check_source(source: &str, path_commands: &[String]) -> Vec<Finding> {
    check_script(source, path_commands, None)
}

pub fn check_script(
    source: &str,
    path_commands: &[String],
    script_directory: Option<&Path>,
) -> Vec<Finding> {
    let mut lines: Vec<String> = source.lines().map(String::from).collect();
    let mut findings = text_findings(&mut lines);
    let script = parse_script(&lines, &mut findings);
    findings.retain(|finding| finding.line <= script.last_line);

    let known = known_names(path_commands, &script.list);
    let mut functions = BTreeSet::new();
    defined_functions(&script.list, &mut functions);
    let mut top_definitions = HashMap::new();
    let mut nested_definitions = BTreeSet::new();
    record_definitions(
        &script.list,
        false,
        &mut top_definitions,
        &mut nested_definitions,
    );
    let mut sources = Sources {
        script_directory: script_directory.map(Path::to_path_buf),
        ..Sources::default()
    };
    let mut sourced = BTreeSet::new();
    sources.scan(&script.list, 0, &mut sourced);
    sources.functions = sourced;
    sources.visited.clear();
    let mut checker = Checker {
        known: &known,
        functions: &functions,
        top_definitions,
        nested_definitions,
        defined: BTreeSet::new(),
        sources,
        findings,
    };
    checker.list(&script.list, &Scope::default());
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
    let path_commands = commands::on_path(&crate::bytes::env_var("PATH").unwrap_or_default());
    let mut status = 0;
    for path in paths {
        let source = match crate::bytes::read_file(path) {
            Ok(source) => source,
            Err(error) => {
                crate::shell::error(format!("{path}: {}", crate::sys::error_message(&error)));
                status = 2;
                continue;
            }
        };
        let directory = Path::new(path).parent().map(|parent| {
            if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            }
        });
        let findings = check_script(&source, &path_commands, directory);
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
        let findings = check("echo start\nmystery-tool --flag\nbind -x echo\n");
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
        assert!(findings.iter().any(|finding| finding.0 == 4
            && !finding.1
            && finding.2.contains("only exists in zsh")));
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
    fn functions_defined_later_still_count_inside_functions() {
        assert!(check("main() { helper; }\nhelper() { echo; }\nmain\n").is_empty());
    }

    #[test]
    fn top_level_calls_before_a_definition_are_errors() {
        let findings = check("early\nearly() { echo; }\nearly\n");
        assert_eq!(
            findings,
            [(
                1,
                true,
                "'early' is called before it is defined on line 2".to_string()
            )]
        );
    }

    #[test]
    fn checking_stops_after_a_top_level_exit() {
        assert!(check("echo hi\nexit 0\nthis is not { shell (\n").is_empty());
        assert!(check("exec ls\n)))\n").is_empty());
        assert!(!check("f() { exit; }\n)))\n").is_empty());
        assert!(!check("true || exit\n)))\n").is_empty());
        assert!(!check("exec >/dev/null\n)))\n").is_empty());
    }

    #[test]
    fn unclosed_quotes_point_at_their_start() {
        let findings = check("echo a\necho \"unclosed\necho b\necho c\n");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].0, 2);
        assert!(findings[0].2.contains("unexpected end of file"));
    }

    #[test]
    fn guarded_zsh_and_optional_commands_are_fine() {
        assert!(
            check("if [ -n \"$ZSH_VERSION\" ]; then autoload -U compinit; compinit; fi\n")
                .is_empty()
        );
        assert!(check("[ -n \"$ZSH_VERSION\" ] && setopt autocd\n").is_empty());
        assert!(check("if command -v fancy >/dev/null; then fancy --go; fi\n").is_empty());
        assert!(check("command -v fancy >/dev/null && fancy --go\n").is_empty());
        assert!(
            check("hash -r\nalias ll='ls -l'\nll\n")
                .iter()
                .all(|finding| !finding.1 && finding.2.contains("'hash'"))
        );
    }

    #[test]
    fn unsupported_features_are_named() {
        let messages = |source: &str| -> Vec<String> {
            check(source).into_iter().map(|finding| finding.2).collect()
        };
        assert!(messages("trap 'echo x' DEBUG\n").is_empty());
        assert!(messages("declare -rn ref=x\n")[0].contains("name references"));
        assert!(messages("echo ${BASH_VERSINFO[0]}\n")[0].contains("BASH_VERSINFO"));
        assert!(messages("printf '%(%Y)T' -1\n").is_empty());
        assert!(messages("select x in a b; do echo $x; done\n").is_empty());
        assert!(messages("shopt -s extglob\necho @(a|b)\n").is_empty());
    }

    #[test]
    fn sourced_files_define_functions() {
        let directory = std::env::temp_dir().join(format!("grfsh_check_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("lib.sh"), "libfn() { echo lib; }\n").unwrap();
        let path = vec!["ls".to_string()];
        for source in [
            ". ./lib.sh\nlibfn\n",
            "source \"$(dirname \"$0\")/lib.sh\"\nlibfn\n",
            "here=$(dirname \"$0\")\n. \"$here/lib.sh\"\nlibfn\n",
            ". \"${BASH_SOURCE%/*}/lib.sh\"\nlibfn\n",
        ] {
            let findings = check_script(source, &path, Some(&directory));
            assert!(findings.is_empty(), "{source}: {findings:?}");
        }
        let unresolved = check_script(". \"$LIB\"\nlibfn\nother\n", &path, Some(&directory));
        assert!(unresolved.is_empty(), "{unresolved:?}");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
