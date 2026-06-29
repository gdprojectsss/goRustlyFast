use std::borrow::Cow;

use rustyline::Context;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::SearchDirection;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};

use crate::syntax::parser;

use super::completion::{self, ShellNames, Target};
use super::highlight::{self, DIM, RESET};

pub(super) struct ShellHelper {
    file_completer: FilenameCompleter,
    names: ShellNames,
}

impl ShellHelper {
    pub(super) fn new() -> Self {
        ShellHelper {
            file_completer: FilenameCompleter::new(),
            names: ShellNames::default(),
        }
    }

    pub(super) fn update(&mut self, mut names: ShellNames) {
        names.commands.sort();
        names.commands.dedup();
        self.names = names;
    }

    fn is_command(&self, name: &str) -> bool {
        self.names
            .commands
            .binary_search_by(|candidate| candidate.as_str().cmp(name))
            .is_ok()
    }

    fn files(
        &self,
        line: &str,
        position: usize,
        context: &Context<'_>,
        directories_only: bool,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, mut pairs) = self.file_completer.complete(line, position, context)?;
        if directories_only {
            pairs.retain(|pair| pair.replacement.ends_with('/'));
        }
        Ok((start, pairs))
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        position: usize,
        context: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let request = completion::analyze(&line[..position]);
        let target = completion::target(&request);
        match target {
            Target::Files => self.files(line, position, context, false),
            Target::Directories => self.files(line, position, context, true),
            Target::WordsAndFiles(_) => {
                let words = completion::candidates(&target, &request.current, &self.names);
                let (start, files) = self.files(line, position, context, false)?;
                if start == request.start {
                    Ok((start, words.into_iter().chain(files).collect()))
                } else if words.is_empty() {
                    Ok((start, files))
                } else {
                    Ok((request.start, words))
                }
            }
            _ => Ok((
                request.start,
                completion::candidates(&target, &request.current, &self.names),
            )),
        }
    }
}

impl Highlighter for ShellHelper {
    fn highlight<'l>(&self, line: &'l str, _position: usize) -> Cow<'l, str> {
        if line.is_empty() {
            return line.into();
        }
        highlight::highlight(line, |name| self.is_command(name)).into()
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        format!("{DIM}{hint}{RESET}").into()
    }

    fn highlight_char(&self, _line: &str, _position: usize, _kind: CmdKind) -> bool {
        true
    }
}

impl Hinter for ShellHelper {
    type Hint = String;

    fn hint(&self, line: &str, position: usize, context: &Context<'_>) -> Option<String> {
        if line.is_empty() || position < line.len() {
            return None;
        }
        let history = context.history();
        (0..history.len()).rev().find_map(|index| {
            let entry = history.get(index, SearchDirection::Forward).ok()??;
            (entry.entry.starts_with(line) && entry.entry.len() > line.len())
                .then(|| entry.entry[line.len()..].to_string())
        })
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
