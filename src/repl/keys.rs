use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rustyline::{
    Cmd, ConditionalEventHandler, Event, EventContext, KeyCode, KeyEvent, Modifiers, Movement,
    RepeatCount, Word,
};

use crate::signals;

use super::git::REDRAW_SIGNAL;

pub(crate) const ACTIONS: &[&str] = &[
    "accept-line",
    "accept-hint",
    "backward-char",
    "backward-kill-word",
    "backward-word",
    "beginning-of-line",
    "capitalize-word",
    "clear-screen",
    "complete",
    "downcase-word",
    "end-of-line",
    "forward-char",
    "forward-word",
    "history-search-backward",
    "history-search-forward",
    "kill-line",
    "kill-whole-line",
    "next-history",
    "previous-history",
    "reverse-search-history",
    "transpose-chars",
    "undo",
    "upcase-word",
    "yank",
];

pub(crate) fn action(name: &str) -> Option<Cmd> {
    Some(match name {
        "accept-line" => Cmd::AcceptLine,
        "accept-hint" => Cmd::CompleteHint,
        "backward-char" => Cmd::Move(Movement::BackwardChar(1)),
        "backward-kill-word" => Cmd::Kill(Movement::BackwardWord(1, Word::Emacs)),
        "backward-word" => Cmd::Move(Movement::BackwardWord(1, Word::Emacs)),
        "beginning-of-line" => Cmd::Move(Movement::BeginningOfLine),
        "capitalize-word" => Cmd::CapitalizeWord,
        "clear-screen" => Cmd::ClearScreen,
        "complete" => Cmd::Complete,
        "downcase-word" => Cmd::DowncaseWord,
        "end-of-line" => Cmd::Move(Movement::EndOfLine),
        "forward-char" => Cmd::Move(Movement::ForwardChar(1)),
        "forward-word" => Cmd::Move(Movement::ForwardWord(
            1,
            rustyline::At::AfterEnd,
            Word::Emacs,
        )),
        "history-search-backward" => Cmd::HistorySearchBackward,
        "history-search-forward" => Cmd::HistorySearchForward,
        "kill-line" => Cmd::Kill(Movement::EndOfLine),
        "kill-whole-line" => Cmd::Kill(Movement::WholeLine),
        "next-history" => Cmd::NextHistory,
        "previous-history" => Cmd::PreviousHistory,
        "reverse-search-history" => Cmd::ReverseSearchHistory,
        "transpose-chars" => Cmd::TransposeChars,
        "undo" => Cmd::Undo(1),
        "upcase-word" => Cmd::UpcaseWord,
        "yank" => Cmd::Yank(1, rustyline::Anchor::Before),
        _ => return None,
    })
}

pub(crate) fn key(spec: &str) -> Option<KeyEvent> {
    let named = |name: &str| -> Option<KeyCode> {
        Some(match name.to_ascii_lowercase().as_str() {
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "tab" => KeyCode::Tab,
            "enter" => KeyCode::Enter,
            "delete" => KeyCode::Delete,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            function if function.len() > 1 && function.starts_with('f') => KeyCode::F(
                function[1..]
                    .parse()
                    .ok()
                    .filter(|number| (1..=24).contains(number))?,
            ),
            _ => return None,
        })
    };
    let (alt, rest) = match spec
        .strip_prefix("\\e")
        .or_else(|| spec.strip_prefix("^["))
        .or_else(|| spec.strip_prefix("alt-"))
    {
        Some(rest) => (true, rest),
        None => (false, spec),
    };
    let modifiers = if alt { Modifiers::ALT } else { Modifiers::NONE };
    if let Some(letter) = rest
        .strip_prefix('^')
        .or_else(|| rest.strip_prefix("ctrl-"))
    {
        let mut chars = letter.chars();
        let character = chars.next()?.to_ascii_lowercase();
        if chars.next().is_some() {
            return None;
        }
        return Some(KeyEvent(
            KeyCode::Char(character),
            modifiers | Modifiers::CTRL,
        ));
    }
    if let Some(code) = named(rest) {
        return Some(KeyEvent(code, modifiers));
    }
    let mut chars = rest.chars();
    let character = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(KeyEvent(KeyCode::Char(character), modifiers))
}

pub(super) fn expand_first_word(
    line: &str,
    abbreviations: &BTreeMap<String, String>,
) -> Option<String> {
    let trimmed = line.trim_start();
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let expansion = abbreviations.get(&trimmed[..end])?;
    Some(format!("{expansion}{}", &trimmed[end..]))
}

pub(super) fn keep_prompt_on_screen() {
    signals::set_wake_input(REDRAW_SIGNAL, None);
}

pub(super) struct WatchKeys {
    pub interrupted: Arc<AtomicBool>,
}

impl ConditionalEventHandler for WatchKeys {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, _: &EventContext) -> Option<Cmd> {
        keep_prompt_on_screen();
        self.interrupted.store(false, Ordering::SeqCst);
        None
    }
}

pub(super) struct InterruptLine {
    pub interrupted: Arc<AtomicBool>,
}

impl ConditionalEventHandler for InterruptLine {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, _: &EventContext) -> Option<Cmd> {
        keep_prompt_on_screen();
        self.interrupted.store(true, Ordering::SeqCst);
        Some(Cmd::AcceptLine)
    }
}

fn suggestion<'a>(context: &'a EventContext) -> Option<&'a str> {
    if context.pos() != context.line().len() {
        return None;
    }
    context.hint_text().filter(|text| !text.trim().is_empty())
}

fn first_word(text: &str) -> &str {
    let start = text.len() - text.trim_start().len();
    let end = text[start..]
        .find(char::is_whitespace)
        .map_or(text.len(), |offset| start + offset);
    &text[..end]
}

pub(super) struct AcceptSuggestion {
    pub whole: bool,
}

impl ConditionalEventHandler for AcceptSuggestion {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, context: &EventContext) -> Option<Cmd> {
        keep_prompt_on_screen();
        let text = suggestion(context)?;
        Some(if self.whole {
            Cmd::CompleteHint
        } else {
            Cmd::Insert(1, first_word(text).to_string())
        })
    }
}

struct Yanked {
    offset: usize,
    line: String,
    position: usize,
    inserted: usize,
}

pub(super) struct YankLastArgument {
    arguments: Arc<Mutex<Vec<String>>>,
    previous: Mutex<Option<Yanked>>,
}

impl YankLastArgument {
    pub(super) fn new(arguments: Arc<Mutex<Vec<String>>>) -> Self {
        YankLastArgument {
            arguments,
            previous: Mutex::new(None),
        }
    }
}

fn start_before(line: &str, position: usize, characters: usize) -> usize {
    if characters == 0 {
        return position;
    }
    line[..position]
        .char_indices()
        .rev()
        .nth(characters - 1)
        .map_or(0, |(index, _)| index)
}

impl ConditionalEventHandler for YankLastArgument {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, context: &EventContext) -> Option<Cmd> {
        keep_prompt_on_screen();
        let arguments = self.arguments.lock().ok()?;
        let mut previous = self.previous.lock().ok()?;
        let line = context.line();
        let position = context.pos();
        let (offset, replaced) = match previous.as_ref() {
            Some(yanked) if yanked.line == line && yanked.position == position => {
                (yanked.offset + 1, yanked.inserted)
            }
            _ => (0, 0),
        };
        let Some(word) = arguments.iter().rev().nth(offset).cloned() else {
            return Some(Cmd::Noop);
        };
        let start = start_before(line, position, replaced);
        *previous = Some(Yanked {
            offset,
            line: format!("{}{word}{}", &line[..start], &line[position..]),
            position: start + word.len(),
            inserted: word.chars().count(),
        });
        Some(if replaced > 0 {
            let line_start = line[..start].rfind('\n').map_or(0, |newline| newline + 1);
            let text = format!("{}{word}", &line[line_start..start]);
            Cmd::Replace(Movement::BeginningOfLine, Some(text))
        } else {
            Cmd::Insert(1, word)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_specs() {
        assert_eq!(
            key("^G"),
            Some(KeyEvent(KeyCode::Char('g'), Modifiers::CTRL))
        );
        assert_eq!(
            key("\\ef"),
            Some(KeyEvent(KeyCode::Char('f'), Modifiers::ALT))
        );
        assert_eq!(
            key("ctrl-x"),
            Some(KeyEvent(KeyCode::Char('x'), Modifiers::CTRL))
        );
        assert_eq!(key("F5"), Some(KeyEvent(KeyCode::F(5), Modifiers::NONE)));
        assert_eq!(key("up"), Some(KeyEvent(KeyCode::Up, Modifiers::NONE)));
        assert!(key("^GG").is_none());
        assert!(key("nonsense").is_none());
    }

    #[test]
    fn every_listed_action_maps_to_a_command() {
        for name in ACTIONS {
            assert!(action(name).is_some(), "{name}");
        }
        assert!(action("fly").is_none());
    }

    #[test]
    fn suggestion_words() {
        assert_eq!(first_word("status --short"), "status");
        assert_eq!(first_word(" --short now"), " --short");
        assert_eq!(first_word("tail"), "tail");
    }

    #[test]
    fn yank_start_counts_characters() {
        assert_eq!(start_before("echo héllo", 11, 5), 5);
        assert_eq!(start_before("echo x", 6, 0), 6);
    }

    #[test]
    fn first_word_expansion() {
        let abbreviations = BTreeMap::from([("gco".to_string(), "git checkout".to_string())]);
        assert_eq!(
            expand_first_word("gco main", &abbreviations).as_deref(),
            Some("git checkout main")
        );
        assert_eq!(expand_first_word("echo gco", &abbreviations), None);
    }
}
