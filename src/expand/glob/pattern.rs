use std::collections::HashSet;

#[derive(Debug, Clone)]
enum ClassItem {
    Single(char),
    Range(char, char),
    Named(fn(char) -> bool),
}

#[derive(Debug, Clone)]
struct Class {
    negate: bool,
    items: Vec<ClassItem>,
}

impl Class {
    fn matches(&self, target: char) -> bool {
        let found = self.items.iter().any(|item| match item {
            ClassItem::Single(member) => *member == target,
            ClassItem::Range(low, high) => *low <= target && target <= *high,
            ClassItem::Named(test) => test(target),
        });
        found != self.negate
    }
}

#[derive(Debug, Clone)]
enum Node {
    Char(char),
    Any,
    Star,
    Class(Class),
    Group {
        kind: char,
        alternatives: Vec<Vec<Node>>,
    },
}

impl Node {
    fn matches_char(&self, target: char) -> bool {
        match self {
            Node::Char(character) => *character == target,
            Node::Any => true,
            Node::Class(class) => class.matches(target),
            Node::Star | Node::Group { .. } => false,
        }
    }
}

fn character_class(name: &str) -> Option<fn(char) -> bool> {
    Some(match name {
        "alpha" => char::is_alphabetic,
        "digit" => |character: char| character.is_ascii_digit(),
        "alnum" => char::is_alphanumeric,
        "space" => char::is_whitespace,
        "blank" => |character: char| character == ' ' || character == '\t',
        "upper" => char::is_uppercase,
        "lower" => char::is_lowercase,
        "punct" => |character: char| character.is_ascii_punctuation(),
        "xdigit" => |character: char| character.is_ascii_hexdigit(),
        "cntrl" => char::is_control,
        "print" => |character: char| !character.is_control(),
        "graph" => |character: char| !character.is_control() && !character.is_whitespace(),
        _ => return None,
    })
}

fn bracket(chars: &[char], start: usize) -> Option<(Class, usize)> {
    let mut index = start + 1;
    let negate = matches!(chars.get(index), Some('!' | '^'));
    if negate {
        index += 1;
    }
    let mut items = Vec::new();
    let mut first = true;
    while index < chars.len() {
        let mut member = chars[index];
        if member == ']' && !first {
            return Some((Class { negate, items }, index + 1));
        }
        first = false;
        if member == '[' && chars.get(index + 1) == Some(&':') {
            let name_start = index + 2;
            if let Some(length) = chars[name_start..]
                .windows(2)
                .position(|pair| pair == [':', ']'])
            {
                let name: String = chars[name_start..name_start + length].iter().collect();
                if let Some(test) = character_class(&name) {
                    items.push(ClassItem::Named(test));
                    index = name_start + length + 2;
                    continue;
                }
            }
        }
        if member == '\\' && index + 1 < chars.len() {
            index += 1;
            member = chars[index];
        }
        if index + 2 < chars.len() && chars[index + 1] == '-' && chars[index + 2] != ']' {
            items.push(ClassItem::Range(member, chars[index + 2]));
            index += 3;
        } else {
            items.push(ClassItem::Single(member));
            index += 1;
        }
    }
    None
}

struct Compiler<'a> {
    chars: &'a [char],
    extended: bool,
}

impl Compiler<'_> {
    fn sequence(&self, mut index: usize, in_group: bool) -> Option<(Vec<Node>, usize)> {
        let mut nodes = Vec::new();
        while index < self.chars.len() {
            let character = self.chars[index];
            if in_group && matches!(character, '|' | ')') {
                return Some((nodes, index));
            }
            if self.extended
                && matches!(character, '@' | '?' | '*' | '+' | '!')
                && self.chars.get(index + 1) == Some(&'(')
                && let Some((alternatives, next)) = self.group(index + 2)
            {
                nodes.push(Node::Group {
                    kind: character,
                    alternatives,
                });
                index = next;
                continue;
            }
            match character {
                '\\' if index + 1 < self.chars.len() => {
                    nodes.push(Node::Char(self.chars[index + 1]));
                    index += 2;
                }
                '*' => {
                    if !matches!(nodes.last(), Some(Node::Star)) {
                        nodes.push(Node::Star);
                    }
                    index += 1;
                }
                '?' => {
                    nodes.push(Node::Any);
                    index += 1;
                }
                '[' => match bracket(self.chars, index) {
                    Some((class, next)) => {
                        nodes.push(Node::Class(class));
                        index = next;
                    }
                    None => {
                        nodes.push(Node::Char('['));
                        index += 1;
                    }
                },
                other => {
                    nodes.push(Node::Char(other));
                    index += 1;
                }
            }
        }
        if in_group {
            return None;
        }
        Some((nodes, index))
    }

    fn group(&self, mut index: usize) -> Option<(Vec<Vec<Node>>, usize)> {
        let mut alternatives = Vec::new();
        loop {
            let (nodes, next) = self.sequence(index, true)?;
            alternatives.push(nodes);
            match self.chars.get(next) {
                Some('|') => index = next + 1,
                Some(')') => return Some((alternatives, next + 1)),
                _ => return None,
            }
        }
    }
}

type Memo = HashSet<(u8, usize, usize, usize, usize)>;

fn key(tag: u8, nodes: &[Node], text: &[char]) -> (u8, usize, usize, usize, usize) {
    (
        tag,
        nodes.as_ptr() as usize,
        nodes.len(),
        text.as_ptr() as usize,
        text.len(),
    )
}

fn any_alternative(alternatives: &[Vec<Node>], text: &[char], memo: &mut Memo) -> bool {
    alternatives
        .iter()
        .any(|alternative| sequence_matches(alternative, text, memo))
}

fn repeat_matches(
    alternatives: &[Vec<Node>],
    rest: &[Node],
    text: &[char],
    memo: &mut Memo,
) -> bool {
    let id = key(1, rest, text);
    if memo.contains(&id) {
        return false;
    }
    let found = sequence_matches(rest, text, memo)
        || (1..=text.len()).any(|end| {
            any_alternative(alternatives, &text[..end], memo)
                && repeat_matches(alternatives, rest, &text[end..], memo)
        });
    if !found {
        memo.insert(id);
    }
    found
}

fn group_matches(
    kind: char,
    alternatives: &[Vec<Node>],
    rest: &[Node],
    text: &[char],
    memo: &mut Memo,
) -> bool {
    match kind {
        '@' => (0..=text.len()).any(|end| {
            any_alternative(alternatives, &text[..end], memo)
                && sequence_matches(rest, &text[end..], memo)
        }),
        '?' => {
            sequence_matches(rest, text, memo)
                || (0..=text.len()).any(|end| {
                    any_alternative(alternatives, &text[..end], memo)
                        && sequence_matches(rest, &text[end..], memo)
                })
        }
        '*' => repeat_matches(alternatives, rest, text, memo),
        '+' => (0..=text.len()).any(|end| {
            any_alternative(alternatives, &text[..end], memo)
                && repeat_matches(alternatives, rest, &text[end..], memo)
        }),
        _ => (0..=text.len()).any(|end| {
            !any_alternative(alternatives, &text[..end], memo)
                && sequence_matches(rest, &text[end..], memo)
        }),
    }
}

fn sequence_matches(nodes: &[Node], text: &[char], memo: &mut Memo) -> bool {
    let Some((node, rest)) = nodes.split_first() else {
        return text.is_empty();
    };
    let id = key(0, nodes, text);
    if memo.contains(&id) {
        return false;
    }
    let found = match node {
        Node::Star => (0..=text.len()).any(|skip| sequence_matches(rest, &text[skip..], memo)),
        Node::Group { kind, alternatives } => group_matches(*kind, alternatives, rest, text, memo),
        single => text.split_first().is_some_and(|(first, after)| {
            single.matches_char(*first) && sequence_matches(rest, after, memo)
        }),
    };
    if !found {
        memo.insert(id);
    }
    found
}

#[derive(Debug, Clone)]
pub struct Pattern {
    nodes: Vec<Node>,
    extended: bool,
}

impl Pattern {
    pub fn new(pattern: &str, extglob: bool) -> Pattern {
        let chars: Vec<char> = pattern.chars().collect();
        let compiler = Compiler {
            chars: &chars,
            extended: extglob,
        };
        let nodes = compiler
            .sequence(0, false)
            .map(|(nodes, _)| nodes)
            .unwrap_or_default();
        let extended = nodes.iter().any(|node| matches!(node, Node::Group { .. }));
        Pattern { nodes, extended }
    }

    pub fn literal(&self) -> Option<String> {
        self.nodes
            .iter()
            .map(|node| match node {
                Node::Char(character) => Some(*character),
                _ => None,
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn reversed(&self) -> Pattern {
        Pattern {
            nodes: self.nodes.iter().rev().cloned().collect(),
            extended: self.extended,
        }
    }

    pub fn matches_chars(&self, text: &[char]) -> bool {
        if self.extended {
            return sequence_matches(&self.nodes, text, &mut Memo::new());
        }
        let mut states = self.start_states();
        let mut next = vec![false; states.len()];
        for &character in text {
            if !self.advance(&states, character, &mut next) {
                return false;
            }
            std::mem::swap(&mut states, &mut next);
        }
        states[self.nodes.len()]
    }

    pub fn matches(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        self.matches_chars(&chars)
    }

    fn close(&self, states: &mut [bool]) {
        for (index, node) in self.nodes.iter().enumerate() {
            if states[index] && matches!(node, Node::Star) {
                states[index + 1] = true;
            }
        }
    }

    fn start_states(&self) -> Vec<bool> {
        let mut states = vec![false; self.nodes.len() + 1];
        states[0] = true;
        self.close(&mut states);
        states
    }

    fn advance(&self, states: &[bool], character: char, next: &mut [bool]) -> bool {
        next.fill(false);
        let mut alive = false;
        for (index, node) in self.nodes.iter().enumerate() {
            if !states[index] {
                continue;
            }
            if matches!(node, Node::Star) {
                next[index] = true;
                alive = true;
            } else if node.matches_char(character) {
                next[index + 1] = true;
                alive = true;
            }
        }
        self.close(next);
        alive
    }

    pub fn match_ends(&self, text: &[char], start: usize, longest: bool) -> Option<usize> {
        if self.extended {
            let mut ends = start..=text.len();
            let mut test = |end: &usize| self.matches_chars(&text[start..*end]);
            return if longest {
                ends.rev().find(test)
            } else {
                ends.find(&mut test)
            };
        }
        let mut states = self.start_states();
        let mut next = vec![false; states.len()];
        let accept = self.nodes.len();
        let mut found = states[accept].then_some(start);
        if found.is_some() && !longest {
            return found;
        }
        for (offset, &character) in text[start..].iter().enumerate() {
            if !self.advance(&states, character, &mut next) {
                break;
            }
            std::mem::swap(&mut states, &mut next);
            if states[accept] {
                found = Some(start + offset + 1);
                if !longest {
                    break;
                }
            }
        }
        found
    }

    pub fn suffix_start(&self, text: &[char], longest: bool) -> Option<usize> {
        if self.extended {
            let mut starts = 0..=text.len();
            let mut test = |start: &usize| self.matches_chars(&text[*start..]);
            return if longest {
                starts.find(&mut test)
            } else {
                starts.rev().find(test)
            };
        }
        let reversed_text: Vec<char> = text.iter().rev().copied().collect();
        self.reversed()
            .match_ends(&reversed_text, 0, longest)
            .map(|length| text.len() - length)
    }

    pub fn first_char(&self) -> Option<char> {
        match self.nodes.first() {
            Some(Node::Char(character)) => Some(*character),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extended(pattern: &str, text: &str) -> bool {
        Pattern::new(pattern, true).matches(text)
    }

    #[test]
    fn extended_groups() {
        assert!(extended("@(a|b)", "b"));
        assert!(!extended("@(a|b)", "c"));
        assert!(extended("?(x)", ""));
        assert!(!extended("?(x)", "xx"));
        assert!(extended("*(ab)", "abab"));
        assert!(extended("*(ab)", ""));
        assert!(extended("+(ab)", "abab"));
        assert!(!extended("+(ab)", ""));
        assert!(extended("!(*.h)", "foo.c"));
        assert!(!extended("!(*.h)", "foo.h"));
        assert!(!extended("a!(b)c", "abc"));
        assert!(extended("a!(b)c", "axc"));
        assert!(extended("a!(b)c", "ac"));
        assert!(extended("*.@(c|d)", "a.b.c"));
        assert!(extended("+([0-9])", "123"));
        assert!(!extended("+([0-9])", "12a"));
        assert!(extended("@(a|+(b|c))d", "bcbd"));
        assert!(!extended("@(a|+(b|c))d", "abcbd"));
    }

    #[test]
    fn unclosed_groups_are_literal() {
        assert!(extended("@(a", "@(a"));
        assert!(!Pattern::new("@(a)", false).matches("a"));
        assert!(Pattern::new("@(a)", false).matches("@(a)"));
    }

    #[test]
    fn prefix_and_suffix_searches() {
        let text: Vec<char> = "aaabbb".chars().collect();
        let star = Pattern::new("a*", false);
        assert_eq!(star.match_ends(&text, 0, false), Some(1));
        assert_eq!(star.match_ends(&text, 0, true), Some(6));
        assert_eq!(Pattern::new("*b", false).suffix_start(&text, true), Some(0));
        assert_eq!(
            Pattern::new("*b", false).suffix_start(&text, false),
            Some(5)
        );
        assert_eq!(
            Pattern::new("+(a)", true).match_ends(&text, 0, true),
            Some(3)
        );
    }
}
