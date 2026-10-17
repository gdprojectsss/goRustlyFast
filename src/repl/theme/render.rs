use crate::text::display_width;

use super::segments::{PromptContext, Segment, segment};
use super::{Icons, Style, Theme};

const RESET: &str = "\x1b[0m";
const CLASSIC_BACKGROUND: &str = "236";

pub(in crate::repl) struct RenderedPrompt {
    pub left: String,
    pub right: String,
    pub right_width: usize,
    pub transient: String,
}

fn named(name: &str) -> Option<u8> {
    Some(match name {
        "black" => 0,
        "red" => 1,
        "green" => 2,
        "yellow" => 3,
        "blue" => 4,
        "magenta" => 5,
        "cyan" => 6,
        "white" => 7,
        "gray" | "grey" => 8,
        "bright-red" => 9,
        "bright-green" => 10,
        "bright-yellow" => 11,
        "bright-blue" => 12,
        "bright-magenta" => 13,
        "bright-cyan" => 14,
        "bright-white" => 15,
        _ => return None,
    })
}

fn nearest_256(red: u8, green: u8, blue: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |value: u8| {
        LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, level)| level.abs_diff(value))
            .map_or(0, |(index, _)| index as u8)
    };
    16 + 36 * level(red) + 6 * level(green) + level(blue)
}

pub(super) fn color_code(color: &str, background: bool) -> Option<String> {
    color_code_at_depth(color, background, crate::text::truecolor())
}

fn color_code_at_depth(color: &str, background: bool, truecolor: bool) -> Option<String> {
    let layer = if background { 48 } else { 38 };
    if let Some(hex) = color.strip_prefix('#') {
        if hex.len() != 6 {
            return None;
        }
        let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
        let (red, green, blue) = (channel(0..2)?, channel(2..4)?, channel(4..6)?);
        if truecolor {
            return Some(format!("{layer};2;{red};{green};{blue}"));
        }
        return Some(format!("{layer};5;{}", nearest_256(red, green, blue)));
    }
    let index = match color.parse::<u8>() {
        Ok(index) => index,
        Err(_) => named(color)?,
    };
    Some(match index {
        0..=7 => format!("{}", if background { 40 } else { 30 } + index),
        8..=15 => format!("{}", if background { 100 } else { 90 } + index - 8),
        _ => format!("{layer};5;{index}"),
    })
}

fn paint(foreground: Option<&str>, background: Option<&str>, text: &str) -> String {
    let mut codes = Vec::new();
    if let Some(code) = foreground.and_then(|color| color_code(color, false)) {
        codes.push(code);
    }
    if let Some(code) = background.and_then(|color| color_code(color, true)) {
        codes.push(code);
    }
    if codes.is_empty() {
        return text.to_string();
    }
    format!("\x1b[{}m{text}{RESET}", codes.join(";"))
}

fn color_for(theme: &Theme, segment: &Segment) -> String {
    theme
        .colors
        .iter()
        .find(|(name, _)| name == segment.name)
        .map_or_else(|| segment.color.to_string(), |(_, color)| color.clone())
}

fn collect(theme: &Theme, names: &[String], context: &PromptContext) -> Vec<(Segment, String)> {
    names
        .iter()
        .filter_map(|name| segment(name, theme, context))
        .map(|segment| {
            let color = color_for(theme, &segment);
            (segment, color)
        })
        .collect()
}

fn separator(theme: &Theme, right_side: bool) -> &'static str {
    match (theme.icons, right_side) {
        (Icons::Nerd, false) => "\u{e0b1}",
        (Icons::Nerd, true) => "\u{e0b3}",
        (Icons::Unicode, _) => "\u{2502}",
        (Icons::Ascii, _) => "|",
    }
}

fn block_edge(theme: &Theme, right_side: bool) -> Option<&'static str> {
    match (theme.icons, right_side) {
        (Icons::Nerd, false) => Some("\u{e0b0}"),
        (Icons::Nerd, true) => Some("\u{e0b2}"),
        _ => None,
    }
}

fn contrast(color: &str) -> &'static str {
    match color {
        "yellow" | "green" | "cyan" | "white" | "bright-yellow" | "bright-green"
        | "bright-cyan" | "bright-white" => "black",
        _ => "white",
    }
}

fn draw(theme: &Theme, segments: &[(Segment, String)], right_side: bool) -> String {
    if segments.is_empty() {
        return String::new();
    }
    match theme.style {
        Style::Lean => segments
            .iter()
            .map(|(segment, color)| paint(Some(color), None, &segment.text))
            .collect::<Vec<_>>()
            .join(" "),
        Style::Classic => {
            let thin = paint(
                Some("244"),
                Some(CLASSIC_BACKGROUND),
                separator(theme, right_side),
            );
            let body = segments
                .iter()
                .map(|(segment, color)| {
                    paint(
                        Some(color),
                        Some(CLASSIC_BACKGROUND),
                        &format!(" {} ", segment.text),
                    )
                })
                .collect::<Vec<_>>()
                .join(&thin);
            match block_edge(theme, right_side) {
                Some(edge) if right_side => paint(Some(CLASSIC_BACKGROUND), None, edge) + &body,
                Some(edge) => body + &paint(Some(CLASSIC_BACKGROUND), None, edge),
                None => body,
            }
        }
        Style::Rainbow => {
            let mut out = String::new();
            for (index, (segment, color)) in segments.iter().enumerate() {
                let edge = block_edge(theme, right_side);
                if right_side && let Some(edge) = edge {
                    let previous = index
                        .checked_sub(1)
                        .map(|before| segments[before].1.as_str());
                    out.push_str(&paint(Some(color), previous, edge));
                }
                out.push_str(&paint(
                    Some(contrast(color)),
                    Some(color),
                    &format!(" {} ", segment.text),
                ));
                if !right_side && let Some(edge) = edge {
                    let next = segments.get(index + 1).map(|(_, next)| next.as_str());
                    out.push_str(&paint(Some(color), next, edge));
                }
            }
            out
        }
    }
}

fn prompt_char(theme: &Theme, context: &PromptContext) -> String {
    let symbol = theme
        .prompt_char
        .clone()
        .unwrap_or_else(|| match theme.icons {
            Icons::Ascii => ">".into(),
            _ => "\u{276f}".into(),
        });
    let color = if context.status == 0 { "green" } else { "red" };
    paint(Some(color), None, &symbol)
}

pub(in crate::repl) fn render(theme: &Theme, context: &PromptContext) -> RenderedPrompt {
    if theme.icons != Icons::Ascii && !crate::text::unicode_terminal() {
        let ascii = Theme {
            icons: Icons::Ascii,
            ..theme.clone()
        };
        return render(&ascii, context);
    }
    let left = draw(theme, &collect(theme, &theme.left, context), false);
    let right = draw(theme, &collect(theme, &theme.right, context), true);
    let symbol = prompt_char(theme, context);
    let full = match (left.is_empty(), theme.two_lines) {
        (true, _) => format!("{symbol} "),
        (false, true) => format!("{left}\n{symbol} "),
        (false, false) => format!("{left} {symbol} "),
    };
    RenderedPrompt {
        left: full,
        right_width: display_width(&right),
        right,
        transient: format!("{symbol} "),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn strip(text: &str) -> String {
        let mut out = String::new();
        let mut escape = false;
        for character in text.chars() {
            match character {
                '\x1b' => escape = true,
                'm' if escape => escape = false,
                _ if escape => {}
                _ => out.push(character),
            }
        }
        out
    }

    #[test]
    fn colors() {
        assert_eq!(color_code("red", false).as_deref(), Some("31"));
        assert_eq!(color_code("gray", true).as_deref(), Some("100"));
        assert_eq!(color_code("208", false).as_deref(), Some("38;5;208"));
        assert_eq!(
            color_code_at_depth("#ff8800", false, true).as_deref(),
            Some("38;2;255;136;0")
        );
        assert_eq!(
            color_code_at_depth("#ff8800", true, false).as_deref(),
            Some("48;5;208")
        );
        assert!(color_code("teal-ish", false).is_none());
    }

    #[test]
    fn lean_prompt_layout() {
        let theme = Theme {
            icons: Icons::Ascii,
            left: vec!["dir".into()],
            right: vec!["status".into()],
            ..Theme::preset("lean").unwrap()
        };
        let context = PromptContext {
            status: 2,
            duration: Duration::ZERO,
            jobs: 0,
            git: None,
            directory: "/tmp",
        };
        let prompt = render(&theme, &context);
        assert_eq!(strip(&prompt.left), "/tmp > ");
        assert_eq!(strip(&prompt.right), "x 2");
        assert_eq!(prompt.right_width, 3);
        assert_eq!(strip(&prompt.transient), "> ");
    }

    #[test]
    fn two_line_and_rainbow_layouts() {
        let context = PromptContext {
            status: 0,
            duration: Duration::ZERO,
            jobs: 1,
            git: None,
            directory: "/tmp",
        };
        let pure = Theme {
            icons: Icons::Ascii,
            ..Theme::preset("pure").unwrap()
        };
        assert_eq!(strip(&render(&pure, &context).left), "/tmp\n> ");
        let rainbow = Theme::preset("rainbow").unwrap();
        let rendered = render(&rainbow, &context);
        if crate::text::unicode_terminal() {
            assert!(rendered.left.contains("\u{e0b0}"), "{:?}", rendered.left);
        }
        assert!(strip(&rendered.right).contains("1"));
    }
}
