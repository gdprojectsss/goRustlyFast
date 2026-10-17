use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rustyline::Context;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::{Hint, Hinter};
use rustyline::history::SearchDirection;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};

use crate::syntax::parser;
use crate::text::display_width;

use super::completion::{self, ShellNames, Target};
use super::highlight::{self, DIM, RESET};
use super::keys;

pub(super) const INTERRUPT_MARK: &str = "^C";

pub(super) struct RightPrompt {
    pub colored: String,
    pub width: usize,
    pub left_width: usize,
    pub columns: usize,
}

pub(super) struct ShellHint {
    display: String,
    completion: Option<String>,
}

impl Hint for ShellHint {
    fn display(&self) -> &str {
        &self.display
    }

    fn completion(&self) -> Option<&str> {
        self.completion.as_deref()
    }
}

pub(super) struct ShellHelper {
    interrupted: Arc<AtomicBool>,
    names: ShellNames,
    right_prompt: Option<RightPrompt>,
    shown_right: RefCell<Option<(String, String)>>,
}

impl ShellHelper {
    pub(super) fn new() -> Self {
        ShellHelper {
            interrupted: Arc::new(AtomicBool::new(false)),
            names: ShellNames::default(),
            right_prompt: None,
            shown_right: RefCell::new(None),
        }
    }

    pub(super) fn update(&mut self, mut names: ShellNames) {
        names.commands.sort();
        names.commands.dedup();
        self.names = names;
    }

    pub(super) fn interrupted(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.interrupted)
    }

    pub(super) fn set_right_prompt(&mut self, right_prompt: Option<RightPrompt>) {
        self.right_prompt = right_prompt;
    }

    pub(super) fn is_command(&self, name: &str) -> bool {
        self.names
            .commands
            .binary_search_by(|candidate| candidate.as_str().cmp(name))
            .is_ok()
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        position: usize,
        _context: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let request = completion::analyze(&line[..position]);
        let raw = &line[request.start..position];
        let target = completion::target_with(&request, &self.names);
        let pairs = match target {
            Target::Files => completion::files(raw, false),
            Target::Directories => completion::files(raw, true),
            Target::WordsAndFiles(_) | Target::WordsAndDirectories(_) => {
                let directories_only = matches!(target, Target::WordsAndDirectories(_));
                let mut words = completion::candidates(&target, &request.current, &self.names);
                words.extend(completion::files(raw, directories_only));
                words
            }
            _ => completion::candidates(&target, &request.current, &self.names),
        };
        Ok((request.start, completion::finish_single(pairs)))
    }
}

impl Highlighter for ShellHelper {
    fn highlight<'l>(&self, line: &'l str, _position: usize) -> Cow<'l, str> {
        if line.is_empty() || !crate::text::colors_enabled() {
            return line.into();
        }
        highlight::highlight(line, |name| self.is_command(name)).into()
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        if hint == INTERRUPT_MARK {
            return hint.into();
        }
        if let Some((plain, colored)) = self.shown_right.borrow().as_ref()
            && plain == hint
        {
            return crate::text::without_colors(colored.clone()).into();
        }
        if !crate::text::colors_enabled() {
            return hint.into();
        }
        format!("{DIM}{hint}{RESET}").into()
    }

    fn highlight_char(&self, _line: &str, _position: usize, _kind: CmdKind) -> bool {
        true
    }
}

impl Hinter for ShellHelper {
    type Hint = ShellHint;

    fn hint(&self, line: &str, position: usize, context: &Context<'_>) -> Option<ShellHint> {
        if !line.is_empty() {
            keys::keep_prompt_on_screen();
        }
        if position < line.len() {
            return None;
        }
        if !line.is_empty() {
            let history = context.history();
            let suggestion = (0..history.len()).rev().find_map(|index| {
                let entry = history.get(index, SearchDirection::Forward).ok()??;
                (entry.entry.starts_with(line) && entry.entry.len() > line.len())
                    .then(|| entry.entry[line.len()..].to_string())
            });
            if let Some(suggestion) = suggestion {
                return Some(ShellHint {
                    display: suggestion.clone(),
                    completion: Some(suggestion),
                });
            }
        }
        let right = self.right_prompt.as_ref()?;
        if line.contains('\n') {
            return None;
        }
        let used = right.left_width + display_width(line);
        let padding = right.columns.checked_sub(used + right.width + 1)?;
        if padding == 0 {
            return None;
        }
        let spaces = " ".repeat(padding);
        let plain = format!("{spaces}{}", " ".repeat(right.width));
        *self.shown_right.borrow_mut() =
            Some((plain.clone(), format!("{spaces}{}", right.colored)));
        Some(ShellHint {
            display: plain,
            completion: None,
        })
    }
}

impl Validator for ShellHelper {
    fn validate(&self, context: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        if self.interrupted.load(Ordering::SeqCst) {
            return Ok(ValidationResult::Valid(Some(INTERRUPT_MARK.into())));
        }
        match parser::parse(context.input()) {
            Err(error) if error.incomplete => Ok(ValidationResult::Incomplete),
            _ => Ok(ValidationResult::Valid(None)),
        }
    }
}

impl rustyline::Helper for ShellHelper {}
