mod arith;
mod brace;
pub mod glob;

use crate::shell::Shell;
use crate::syntax::ast::{FallbackKind, ParameterOperation, Subscript, Word, WordPart};
use crate::sys;
use crate::variables::Key;

pub fn words(shell: &mut Shell, words: &[Word]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for word in words {
        for expanded in brace::expand(word) {
            let mut fields = Fields::new(true);
            fields.glob_options = glob::GlobOptions {
                dotglob: shell.options.dotglob,
                nocase: shell.options.nocaseglob,
                globstar: shell.options.globstar,
            };
            fields.nullglob = shell.options.nullglob;
            fields.ifs = shell
                .get_variable("IFS")
                .unwrap_or_else(|| DEFAULT_IFS.into());
            expand_into(shell, &expanded, &mut fields)?;
            out.extend(fields.finish());
        }
    }
    Ok(out)
}

pub fn pattern(shell: &mut Shell, word: &Word) -> Result<String, String> {
    let mut fields = Fields::new(false);
    fields.pattern_mode = true;
    expand_into(shell, word, &mut fields)?;
    Ok(fields.pattern)
}

pub fn regex(shell: &mut Shell, word: &Word) -> Result<String, String> {
    let mut out = String::new();
    for part in &word.parts {
        match part {
            WordPart::Quoted(text) => {
                for character in text.chars() {
                    if "\\^$.|?*+()[]{}".contains(character) {
                        out.push('\\');
                    }
                    out.push(character);
                }
            }
            other => {
                let piece = Word {
                    parts: vec![other.clone()],
                };
                out.push_str(&single(shell, &piece)?);
            }
        }
    }
    Ok(out)
}

pub fn single(shell: &mut Shell, word: &Word) -> Result<String, String> {
    let mut fields = Fields::new(false);
    expand_into(shell, word, &mut fields)?;
    Ok(fields.current)
}

fn expand_into(shell: &mut Shell, word: &Word, fields: &mut Fields) -> Result<(), String> {
    for part in &word.parts {
        match part {
            WordPart::Literal(text) => fields.literal(text),
            WordPart::Quoted(text) => fields.quoted(text),
            WordPart::Tilde(user) => fields.quoted(&tilde(shell, user)),
            WordPart::Parameter {
                name,
                subscript,
                operator,
                quoted,
            } => {
                let joined = name == "*" || matches!(subscript, Some(Subscript::Joined));
                let value = lookup(shell, name, subscript.as_ref())?;
                let value = match operator {
                    Some(operator) => apply(shell, name, subscript.as_ref(), value, operator)?,
                    None if value == Expanded::Missing && shell.options.nounset => {
                        return Err(format!("{name}: unbound variable"));
                    }
                    None => value,
                };
                push_value(fields, value, *quoted, joined);
            }
            WordPart::Length { name, subscript } => {
                let length = match lookup(shell, name, subscript.as_ref())? {
                    Expanded::Missing => 0,
                    Expanded::One(text) => text.chars().count(),
                    Expanded::Many(values) if subscript.is_some() || name == "@" || name == "*" => {
                        values.len()
                    }
                    Expanded::Many(values) => values.first().map_or(0, |text| text.chars().count()),
                };
                fields.quoted(&length.to_string());
            }
            WordPart::Indices { name, quoted } => {
                let keys = match shell.arrays.get(name) {
                    Some(array) => array.keys(),
                    None if shell.get_variable(name).is_some() => vec!["0".into()],
                    None => Vec::new(),
                };
                push_value(fields, Expanded::Many(keys), *quoted, false);
            }
            WordPart::CommandSubstitution { source, quoted } => {
                let out = shell.capture(source);
                if *quoted {
                    fields.quoted(&out);
                } else {
                    fields.split(&out);
                }
            }
            WordPart::ProcessSubstitution { source, output } => {
                let path = shell.process_substitution(source, *output)?;
                fields.quoted(&path);
            }
            WordPart::Arithmetic { expression, .. } => {
                let value = arithmetic(shell, expression)?;
                fields.quoted(&value.to_string());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn brace_expand(word: &Word) -> Vec<Word> {
    brace::expand(word)
}

pub fn evaluate_arithmetic(shell: &mut Shell, text: &str) -> Result<i64, String> {
    arith::eval(shell, text)
}

pub fn arithmetic(shell: &mut Shell, expression: &Word) -> Result<i64, String> {
    let text = single(shell, expression)?;
    arith::eval(shell, &text)
}

#[derive(Debug, Clone, PartialEq)]
enum Expanded {
    Missing,
    One(String),
    Many(Vec<String>),
}

impl Expanded {
    fn is_empty_or_missing(&self) -> bool {
        match self {
            Expanded::Missing => true,
            Expanded::One(text) => text.is_empty(),
            Expanded::Many(values) => values.is_empty(),
        }
    }

    fn map(self, mut transform: impl FnMut(&str) -> String) -> Expanded {
        match self {
            Expanded::Missing => Expanded::One(transform("")),
            Expanded::One(text) => Expanded::One(transform(&text)),
            Expanded::Many(values) => {
                Expanded::Many(values.iter().map(|text| transform(text)).collect())
            }
        }
    }
}

fn push_value(fields: &mut Fields, value: Expanded, quoted: bool, joined: bool) {
    match value {
        Expanded::Missing if quoted && !joined => fields.quoted(""),
        Expanded::Missing => {}
        Expanded::One(text) if quoted => fields.quoted(&text),
        Expanded::One(text) => fields.split(&text),
        Expanded::Many(values) if quoted && joined => fields.quoted(&values.join(" ")),
        Expanded::Many(values) if quoted => {
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    fields.break_field();
                }
                fields.quoted(value);
            }
        }
        Expanded::Many(values) => fields.split(&values.join(" ")),
    }
}

fn lookup(
    shell: &mut Shell,
    name: &str,
    subscript: Option<&Subscript>,
) -> Result<Expanded, String> {
    let positional = name == "@" || name == "*";
    match subscript {
        None if positional => Ok(Expanded::Many(shell.positional.clone())),
        None => Ok(shell
            .get_variable(name)
            .map_or(Expanded::Missing, Expanded::One)),
        Some(Subscript::All | Subscript::Joined) if shell.special_array(name).is_some() => Ok(
            Expanded::Many(shell.special_array(name).unwrap_or_default()),
        ),
        Some(Subscript::Index(word)) if shell.special_array(name).is_some() => {
            let index = arithmetic(shell, word)?;
            let values = shell.special_array(name).unwrap_or_default();
            Ok(usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index).cloned())
                .map_or(Expanded::Missing, Expanded::One))
        }
        Some(Subscript::All | Subscript::Joined) => Ok(match shell.arrays.get(name) {
            Some(array) => Expanded::Many(array.values()),
            None if positional => Expanded::Many(shell.positional.clone()),
            None => shell
                .get_variable(name)
                .map_or(Expanded::Many(Vec::new()), |value| {
                    Expanded::Many(vec![value])
                }),
        }),
        Some(Subscript::Index(word)) => {
            let key = shell.array_key(name, word)?;
            Ok(match shell.arrays.get(name) {
                Some(array) => array
                    .get(&key)
                    .cloned()
                    .map_or(Expanded::Missing, Expanded::One),
                None if key == Key::Index(0) => shell
                    .get_variable(name)
                    .map_or(Expanded::Missing, Expanded::One),
                None => Expanded::Missing,
            })
        }
    }
}

fn assign_fallback(
    shell: &mut Shell,
    name: &str,
    subscript: Option<&Subscript>,
    value: &str,
) -> Result<(), String> {
    match subscript {
        Some(Subscript::Index(word)) => {
            let key = shell.array_key(name, word)?;
            shell.set_element(name, key, value)
        }
        Some(_) => Err(format!("{name}: cannot assign in this way")),
        None => {
            shell.set_variable(name, value);
            Ok(())
        }
    }
}

fn apply(
    shell: &mut Shell,
    name: &str,
    subscript: Option<&Subscript>,
    value: Expanded,
    operator: &ParameterOperation,
) -> Result<Expanded, String> {
    match operator {
        ParameterOperation::Fallback { kind, colon, word } => {
            let unset = match &value {
                Expanded::Missing => true,
                other => *colon && other.is_empty_or_missing(),
            };
            match kind {
                FallbackKind::Default if unset => Ok(Expanded::One(single(shell, word)?)),
                FallbackKind::Assign if unset => {
                    let assigned = single(shell, word)?;
                    assign_fallback(shell, name, subscript, &assigned)?;
                    Ok(Expanded::One(assigned))
                }
                FallbackKind::Alternative if unset => Ok(Expanded::Missing),
                FallbackKind::Alternative => Ok(Expanded::One(single(shell, word)?)),
                FallbackKind::Error if unset => {
                    let message = single(shell, word)?;
                    let message = if message.is_empty() {
                        "parameter null or not set".into()
                    } else {
                        message
                    };
                    Err(format!("{name}: {message}"))
                }
                _ => Ok(value),
            }
        }
        ParameterOperation::RemovePrefix { longest, pattern } => {
            let pattern = self::pattern(shell, pattern)?;
            Ok(value.map(|text| glob::remove_prefix(text, &pattern, *longest)))
        }
        ParameterOperation::RemoveSuffix { longest, pattern } => {
            let pattern = self::pattern(shell, pattern)?;
            Ok(value.map(|text| glob::remove_suffix(text, &pattern, *longest)))
        }
        ParameterOperation::Replace {
            all,
            anchor,
            pattern,
            replacement,
        } => {
            let pattern = self::pattern(shell, pattern)?;
            let replacement = single(shell, replacement)?;
            Ok(value.map(|text| glob::replace(text, &pattern, &replacement, *all, *anchor)))
        }
        ParameterOperation::ChangeCase { upper, all } => Ok(value.map(|text| {
            let convert = |text: &str| {
                if *upper {
                    text.to_uppercase()
                } else {
                    text.to_lowercase()
                }
            };
            if *all {
                return convert(text);
            }
            let mut chars = text.chars();
            match chars.next() {
                Some(first) => convert(&first.to_string()) + chars.as_str(),
                None => String::new(),
            }
        })),
        ParameterOperation::Substring { offset, length } => {
            let offset = arithmetic(shell, offset)?;
            let length = match length {
                Some(word) => Some(arithmetic(shell, word)?),
                None => None,
            };
            Ok(match value {
                Expanded::Many(values) if (name == "@" || name == "*") && subscript.is_none() => {
                    let with_name: Vec<String> =
                        std::iter::once(shell.arg0.clone()).chain(values).collect();
                    Expanded::Many(slice(&with_name, offset, length))
                }
                Expanded::Many(values) => Expanded::Many(slice(&values, offset, length)),
                other => other.map(|text| {
                    let chars: Vec<char> = text.chars().collect();
                    slice(&chars, offset, length).into_iter().collect()
                }),
            })
        }
    }
}

fn slice<T: Clone>(items: &[T], offset: i64, length: Option<i64>) -> Vec<T> {
    let total = items.len() as i64;
    let start = if offset < 0 {
        (total + offset).max(0)
    } else {
        offset.min(total)
    };
    let end = match length {
        None => total,
        Some(length) if length < 0 => (total + length).max(start),
        Some(length) => (start + length).min(total),
    };
    items[start as usize..end.max(start) as usize].to_vec()
}

fn tilde(shell: &Shell, user: &str) -> String {
    let found = match user {
        "" => shell.get_variable("HOME"),
        "+" => shell.get_variable("PWD"),
        "-" => shell.get_variable("OLDPWD"),
        _ => sys::user_home(user),
    };
    found.unwrap_or_else(|| format!("~{user}"))
}

struct Fields {
    split: bool,
    done: Vec<(String, String, bool)>,
    current: String,
    pattern: String,
    glob: bool,
    started: bool,
    pattern_mode: bool,
    glob_options: glob::GlobOptions,
    nullglob: bool,
    ifs: String,
}

pub(crate) const DEFAULT_IFS: &str = " \t\n";

pub(crate) fn split_ifs(text: &str, ifs: &str, limit: Option<usize>) -> Vec<String> {
    let white = |character: char| ifs.contains(character) && character.is_whitespace();
    let hard = |character: char| ifs.contains(character) && !character.is_whitespace();
    let mut fields = Vec::new();
    let mut rest = text.trim_start_matches(white);
    while !rest.is_empty() {
        if limit.is_some_and(|limit| fields.len() + 1 >= limit) {
            fields.push(rest.trim_end_matches(white).to_string());
            break;
        }
        let Some(end) = rest.find(|character: char| ifs.contains(character)) else {
            fields.push(rest.to_string());
            break;
        };
        fields.push(rest[..end].to_string());
        let mut after = rest[end..].trim_start_matches(white);
        if let Some(separator) = after.chars().next().filter(|character| hard(*character)) {
            after = after[separator.len_utf8()..].trim_start_matches(white);
        }
        rest = after;
    }
    fields
}

impl Fields {
    fn new(split: bool) -> Self {
        Fields {
            split,
            done: Vec::new(),
            current: String::new(),
            pattern: String::new(),
            glob: false,
            started: false,
            pattern_mode: false,
            glob_options: glob::GlobOptions::default(),
            nullglob: false,
            ifs: DEFAULT_IFS.to_string(),
        }
    }

    fn literal(&mut self, text: &str) {
        self.started = true;
        self.current.push_str(text);
        if self.split || self.pattern_mode {
            self.pattern.push_str(text);
        }
        if self.split && glob::has_magic(text) {
            self.glob = true;
        }
    }

    fn quoted(&mut self, text: &str) {
        self.started = true;
        self.current.push_str(text);
        if !self.split && !self.pattern_mode {
            return;
        }
        let mut copied = 0;
        for (index, character) in text.char_indices() {
            if matches!(character, '*' | '?' | '[' | ']' | '\\') {
                self.pattern.push_str(&text[copied..index]);
                self.pattern.push('\\');
                copied = index;
            }
        }
        self.pattern.push_str(&text[copied..]);
    }

    fn split(&mut self, text: &str) {
        if self.pattern_mode {
            self.literal(text);
            return;
        }
        if !self.split || self.ifs.is_empty() {
            self.quoted(text);
            return;
        }
        let ifs = self.ifs.clone();
        let separator = |character: char| ifs.contains(character);
        if text.starts_with(separator) {
            self.break_field();
        }
        let pieces = split_ifs(text, &ifs, None);
        let count = pieces.len();
        for (index, piece) in pieces.into_iter().enumerate() {
            self.literal(&piece);
            if index + 1 < count {
                self.break_field();
            }
        }
        if text.ends_with(separator) {
            self.break_field();
        }
    }

    fn break_field(&mut self) {
        if self.started {
            let current = std::mem::take(&mut self.current);
            let pattern = std::mem::take(&mut self.pattern);
            self.done.push((current, pattern, self.glob));
            self.glob = false;
            self.started = false;
        }
    }

    fn finish(mut self) -> Vec<String> {
        self.break_field();
        let mut out = Vec::new();
        for (text, pattern, glob) in self.done {
            match glob
                .then(|| glob::expand(&pattern, self.glob_options))
                .flatten()
            {
                Some(matches) => out.extend(matches),
                None if glob && self.nullglob => {}
                None if glob => out.push(glob::unescape(&pattern)),
                None => out.push(text),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::lexer::{self, Token};

    fn expand(shell: &mut Shell, source: &str) -> Vec<String> {
        let word_list: Vec<Word> = lexer::lex(source)
            .unwrap()
            .tokens
            .into_iter()
            .filter_map(|spanned| match spanned.token {
                Token::Word(word) => Some(word),
                _ => None,
            })
            .collect();
        words(shell, &word_list).unwrap()
    }

    #[test]
    fn unquoted_expansion_splits() {
        let mut shell = Shell::new();
        shell.set_variable("x", "a  b c");
        assert_eq!(expand(&mut shell, "$x"), ["a", "b", "c"]);
        assert_eq!(expand(&mut shell, "\"$x\""), ["a  b c"]);
        assert_eq!(expand(&mut shell, "pre$x"), ["prea", "b", "c"]);
    }

    #[test]
    fn ifs_splitting_rules() {
        let split = |text: &str, ifs: &str| split_ifs(text, ifs, None);
        assert_eq!(split("  a  b  ", DEFAULT_IFS), ["a", "b"]);
        assert_eq!(split("a::b:", ":"), ["a", "", "b"]);
        assert_eq!(split(":a", ":"), ["", "a"]);
        assert_eq!(split("a : b", " :"), ["a", "b"]);
        assert_eq!(split_ifs("x:y:z", ":", Some(2)), ["x", "y:z"]);
        let mut shell = Shell::new();
        shell.set_variable("IFS", ",");
        shell.set_variable("list", "a,b c,d");
        assert_eq!(expand(&mut shell, "$list"), ["a", "b c", "d"]);
    }

    #[test]
    fn empty_unquoted_expansion_vanishes() {
        let mut shell = Shell::new();
        assert_eq!(expand(&mut shell, "$nothing x"), ["x"]);
        assert_eq!(expand(&mut shell, "\"$nothing\""), [""]);
    }

    #[test]
    fn positional_at() {
        let mut shell = Shell::new();
        shell.positional = vec!["a b".into(), "c".into()];
        assert_eq!(expand(&mut shell, "\"$@\""), ["a b", "c"]);
        assert_eq!(expand(&mut shell, "$@"), ["a", "b", "c"]);
        assert_eq!(expand(&mut shell, "\"$*\""), ["a b c"]);
        assert_eq!(expand(&mut shell, "$#"), ["2"]);
    }

    #[test]
    fn parameter_operators() {
        let mut shell = Shell::new();
        shell.set_variable("empty", "");
        assert_eq!(expand(&mut shell, "${unset:-dflt}"), ["dflt"]);
        assert_eq!(expand(&mut shell, "${empty:-dflt}"), ["dflt"]);
        assert_eq!(expand(&mut shell, "${empty-dflt}"), [] as [&str; 0]);
        assert_eq!(expand(&mut shell, "${new:=set}"), ["set"]);
        assert_eq!(shell.get_variable("new").as_deref(), Some("set"));
        assert_eq!(expand(&mut shell, "${new:+alt}"), ["alt"]);
        assert_eq!(expand(&mut shell, "${#new}"), ["3"]);
    }

    #[test]
    fn arithmetic() {
        let mut shell = Shell::new();
        shell.set_variable("i", "41");
        assert_eq!(expand(&mut shell, "$((i + 1))"), ["42"]);
        assert_eq!(expand(&mut shell, "$(( $i * 2 ))"), ["82"]);
    }

    #[test]
    fn quoted_glob_stays_literal() {
        let mut shell = Shell::new();
        assert_eq!(expand(&mut shell, "'src/*.rs'"), ["src/*.rs"]);
        assert!(expand(&mut shell, "src/*.rs").contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn unmatched_glob_is_kept() {
        let mut shell = Shell::new();
        assert_eq!(expand(&mut shell, "nope_*.zzz"), ["nope_*.zzz"]);
    }

    #[test]
    fn tilde() {
        let mut shell = Shell::new();
        shell.variables.insert("HOME".into(), "/home/me".into());
        assert_eq!(expand(&mut shell, "~/x '~'"), ["/home/me/x", "~"]);
    }
}
