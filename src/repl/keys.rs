use std::collections::BTreeMap;

use rustyline::{Cmd, KeyCode, KeyEvent, Modifiers, Movement, Word};

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
    fn first_word_expansion() {
        let abbreviations = BTreeMap::from([("gco".to_string(), "git checkout".to_string())]);
        assert_eq!(
            expand_first_word("gco main", &abbreviations).as_deref(),
            Some("git checkout main")
        );
        assert_eq!(expand_first_word("echo gco", &abbreviations), None);
    }
}
