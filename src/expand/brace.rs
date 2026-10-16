use crate::syntax::ast::{Word, WordPart};

const MAX_RESULTS: usize = 100_000;

#[derive(Debug, Clone, PartialEq)]
enum Item {
    Char(char),
    Part(WordPart),
}

pub fn expand(word: &Word) -> Option<Vec<Word>> {
    let has_brace = word
        .parts
        .iter()
        .any(|part| matches!(part, WordPart::Literal(text) if text.contains('{')));
    if !has_brace {
        return None;
    }
    let mut results = Vec::new();
    expand_items(to_items(word), &mut results);
    Some(results.into_iter().map(from_items).collect())
}

fn to_items(word: &Word) -> Vec<Item> {
    let mut items = Vec::new();
    for part in &word.parts {
        match part {
            WordPart::Literal(text) => items.extend(text.chars().map(Item::Char)),
            other => items.push(Item::Part(other.clone())),
        }
    }
    items
}

fn from_items(items: Vec<Item>) -> Word {
    let mut parts = Vec::new();
    for item in items {
        match item {
            Item::Char(character) => match parts.last_mut() {
                Some(WordPart::Literal(text)) => text.push(character),
                _ => parts.push(WordPart::Literal(character.to_string())),
            },
            Item::Part(part) => parts.push(part),
        }
    }
    Word { parts }
}

fn matching_close(items: &[Item], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (index, item) in items.iter().enumerate().skip(open) {
        match item {
            Item::Char('{') => depth += 1,
            Item::Char('}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn top_level_commas(content: &[Item]) -> Vec<usize> {
    let mut depth = 0;
    let mut commas = Vec::new();
    for (index, item) in content.iter().enumerate() {
        match item {
            Item::Char('{') => depth += 1,
            Item::Char('}') => depth -= 1,
            Item::Char(',') if depth == 0 => commas.push(index),
            _ => {}
        }
    }
    commas
}

fn alternatives(content: &[Item]) -> Option<Vec<Vec<Item>>> {
    let commas = top_level_commas(content);
    if !commas.is_empty() {
        let mut pieces = Vec::with_capacity(commas.len() + 1);
        let mut start = 0;
        for comma in commas {
            pieces.push(content[start..comma].to_vec());
            start = comma + 1;
        }
        pieces.push(content[start..].to_vec());
        return Some(pieces);
    }
    let text: Option<String> = content
        .iter()
        .map(|item| match item {
            Item::Char(character) => Some(*character),
            Item::Part(_) => None,
        })
        .collect();
    sequence(&text?).map(|values| {
        values
            .into_iter()
            .map(|value| value.chars().map(Item::Char).collect())
            .collect()
    })
}

fn sequence(text: &str) -> Option<Vec<String>> {
    let pieces: Vec<&str> = text.split("..").collect();
    let (start, end, step) = match pieces.as_slice() {
        [start, end] => (*start, *end, None),
        [start, end, step] => (*start, *end, Some(step.parse::<i64>().ok()?)),
        _ => return None,
    };
    let step = step.map(i64::abs).filter(|step| *step != 0).unwrap_or(1);

    if let (Ok(first), Ok(last)) = (start.parse::<i64>(), end.parse::<i64>()) {
        let padded = |text: &str| {
            let digits = text.trim_start_matches('-');
            digits.len() > 1 && digits.starts_with('0')
        };
        let width = if padded(start) || padded(end) {
            start.len().max(end.len())
        } else {
            0
        };
        let count = (first.abs_diff(last) / step as u64) as usize + 1;
        if count > MAX_RESULTS {
            return None;
        }
        let direction = if last >= first { step } else { -step };
        return Some(
            (0..count as i64)
                .map(|offset| first + offset * direction)
                .map(|value| format!("{value:0width$}"))
                .collect(),
        );
    }

    let mut start_chars = start.chars();
    let mut end_chars = end.chars();
    let (Some(first), None, Some(last), None) = (
        start_chars.next(),
        start_chars.next(),
        end_chars.next(),
        end_chars.next(),
    ) else {
        return None;
    };
    if !first.is_ascii_alphabetic() || !last.is_ascii_alphabetic() {
        return None;
    }
    let (low, high) = (first.min(last) as u8, first.max(last) as u8);
    let mut letters: Vec<String> = (low..=high)
        .step_by(step as usize)
        .map(|byte| (byte as char).to_string())
        .collect();
    if first > last {
        letters.reverse();
    }
    Some(letters)
}

fn expand_items(items: Vec<Item>, results: &mut Vec<Vec<Item>>) {
    if results.len() >= MAX_RESULTS {
        return;
    }
    let mut search_from = 0;
    while let Some(open) = items[search_from..]
        .iter()
        .position(|item| *item == Item::Char('{'))
        .map(|offset| offset + search_from)
    {
        let Some(close) = matching_close(&items, open) else {
            break;
        };
        if let Some(choices) = alternatives(&items[open + 1..close]) {
            for choice in choices {
                let mut expanded = items[..open].to_vec();
                expanded.extend(choice);
                expanded.extend_from_slice(&items[close + 1..]);
                expand_items(expanded, results);
            }
            return;
        }
        search_from = open + 1;
    }
    results.push(items);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::lexer::{self, Token};

    fn expand_text(source: &str) -> Vec<String> {
        let word = lexer::lex(source)
            .unwrap()
            .tokens
            .into_iter()
            .find_map(|spanned| match spanned.token {
                Token::Word(word) => Some(word),
                _ => None,
            })
            .unwrap();
        expand(&word)
            .unwrap_or_else(|| vec![word.clone()])
            .into_iter()
            .map(|word| {
                word.parts
                    .iter()
                    .map(|part| match part {
                        WordPart::Literal(text) | WordPart::Quoted(text) => text.clone(),
                        _ => "$".into(),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn lists() {
        assert_eq!(
            expand_text("src/{a,b,c}.rs"),
            ["src/a.rs", "src/b.rs", "src/c.rs"]
        );
        assert_eq!(expand_text("{x,y}{1,2}"), ["x1", "x2", "y1", "y2"]);
        assert_eq!(expand_text("a{b,{c,d}}"), ["ab", "ac", "ad"]);
        assert_eq!(expand_text("file{,.bak}"), ["file", "file.bak"]);
    }

    #[test]
    fn sequences() {
        assert_eq!(expand_text("{1..4}"), ["1", "2", "3", "4"]);
        assert_eq!(expand_text("{3..1}"), ["3", "2", "1"]);
        assert_eq!(expand_text("{01..03}"), ["01", "02", "03"]);
        assert_eq!(expand_text("{0..10..5}"), ["0", "5", "10"]);
        assert_eq!(expand_text("{a..e..2}"), ["a", "c", "e"]);
    }

    #[test]
    fn things_that_stay_literal() {
        assert_eq!(expand_text("{}"), ["{}"]);
        assert_eq!(expand_text("{single}"), ["{single}"]);
        assert_eq!(expand_text("'{a,b}'"), ["{a,b}"]);
        assert_eq!(expand_text("{a,b"), ["{a,b"]);
        assert_eq!(expand_text("{1..x}"), ["{1..x}"]);
    }

    #[test]
    fn expansions_inside_alternatives_survive() {
        assert_eq!(expand_text("{$HOME,x}"), ["$", "x"]);
    }
}
