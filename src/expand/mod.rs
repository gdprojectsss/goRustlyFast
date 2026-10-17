mod arith;
mod brace;
pub mod glob;
mod prompt;

use std::borrow::Cow;
use std::rc::Rc;

use crate::shell::Shell;
use crate::syntax::ast::{FallbackKind, ParameterOperation, Subscript, Word, WordPart};
use crate::sys;
use crate::variables::{Key, array_assignment, quote_key, quote_value, quote_word};

thread_local! {
    static DEFAULT_SEPARATORS: Rc<str> = Rc::from(DEFAULT_IFS);
}

fn separators(shell: &Shell) -> Rc<str> {
    match shell.variables.get("IFS") {
        Some(ifs) => Rc::from(ifs.as_str()),
        None => DEFAULT_SEPARATORS.with(Rc::clone),
    }
}

pub fn words(shell: &mut Shell, words: &[Word]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let ifs = separators(shell);
    for word in words {
        let braced = brace::expand(word)?;
        for expanded in braced.as_deref().unwrap_or(std::slice::from_ref(word)) {
            let expanded = argument_tildes(expanded);
            let expanded = expanded.as_ref();
            let mut fields = Fields::new(true);
            fields.glob_options = glob::options(shell);
            fields.nullglob = shell.options.nullglob;
            fields.noglob = shell.options.noglob;
            fields.failglob = shell.options.failglob;
            fields.ifs = Rc::clone(&ifs);
            expand_into(shell, expanded, &mut fields)?;
            out.extend(fields.finish()?);
        }
    }
    Ok(out)
}

pub fn pattern(shell: &mut Shell, word: &Word) -> Result<String, String> {
    glob::configure_locale(shell);
    let mut fields = Fields::new(false);
    fields.pattern_mode = true;
    expand_into(shell, word, &mut fields)?;
    Ok(fields.pattern)
}

fn push_regex_literal(out: &mut String, text: &str) {
    for character in text.chars() {
        if "\\^$.|?*+()[]{}".contains(character) {
            out.push('\\');
        }
        out.push(character);
    }
}

fn quoted_expansion(part: &WordPart) -> bool {
    matches!(
        part,
        WordPart::Parameter { quoted: true, .. }
            | WordPart::Indirect { quoted: true, .. }
            | WordPart::Indices { quoted: true, .. }
            | WordPart::Names { quoted: true, .. }
            | WordPart::CommandSubstitution { quoted: true, .. }
            | WordPart::Arithmetic { quoted: true, .. }
    )
}

pub fn regex(shell: &mut Shell, word: &Word) -> Result<String, String> {
    let mut out = String::new();
    for part in &word.parts {
        match part {
            WordPart::Quoted(text) => push_regex_literal(&mut out, text),
            other => {
                let piece = Word {
                    parts: vec![other.clone()],
                };
                let expanded = single(shell, &piece)?;
                if quoted_expansion(other) {
                    push_regex_literal(&mut out, &expanded);
                } else {
                    out.push_str(&expanded);
                }
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
            WordPart::Literal(text) if fields.split_literals => fields.split(text),
            WordPart::Literal(text) => fields.literal(text),
            WordPart::Quoted(text) => fields.quoted(text),
            WordPart::Tilde(user) => fields.quoted(&tilde(shell, user)),
            WordPart::Parameter {
                name,
                subscript,
                operator,
                quoted,
            } => {
                let value = lookup(shell, name, subscript.as_ref())?;
                let parameter = Parameter {
                    name,
                    subscript: subscript.as_ref(),
                    quoted: *quoted,
                };
                expand_parameter(shell, fields, &parameter, value, operator.as_deref())?;
            }
            WordPart::Length {
                name,
                subscript,
                quoted,
            } => {
                let length = length(shell, name, subscript.as_ref())?.to_string();
                if *quoted {
                    fields.quoted(&length);
                } else {
                    fields.split(&length);
                }
            }
            WordPart::BadSubstitution(text) => {
                shell.flow = crate::shell::Flow::Abort;
                return Err(format!("{text}: bad substitution"));
            }
            WordPart::Indices {
                name,
                quoted,
                joined,
            } => {
                let name = shell.resolve_name(name);
                let keys = match shell.arrays.get(&name) {
                    Some(array) => array.keys(),
                    None if shell.get_variable(&name).is_some() => vec!["0".into()],
                    None => Vec::new(),
                };
                push_value(
                    fields,
                    Expanded::Many(keys),
                    *quoted,
                    &separator(shell, *joined),
                );
            }
            WordPart::Indirect {
                name,
                subscript,
                operator,
                quoted,
            } => {
                if subscript.is_none() && shell.is_reference(name) && operator.is_none() {
                    let target = shell.variables.get(name).cloned().unwrap_or_default();
                    push_value(fields, Expanded::One(target), *quoted, &None);
                    continue;
                }
                let Some(target) = indirect_value(shell, name, subscript.as_ref())? else {
                    continue;
                };
                let (target, subscript) = indirect_target(shell, &target)?;
                let value = lookup(shell, &target, subscript.as_ref())?;
                let parameter = Parameter {
                    name: &target,
                    subscript: subscript.as_ref(),
                    quoted: *quoted,
                };
                expand_parameter(shell, fields, &parameter, value, operator.as_deref())?;
            }
            WordPart::Names {
                prefix,
                joined,
                quoted,
            } => {
                let mut names: Vec<String> = shell
                    .variables
                    .keys()
                    .chain(shell.arrays.keys())
                    .cloned()
                    .chain(std::env::vars_os().filter_map(|(name, _)| name.into_string().ok()))
                    .filter(|name| name.starts_with(prefix.as_str()))
                    .collect();
                names.sort();
                names.dedup();
                push_value(
                    fields,
                    Expanded::Many(names),
                    *quoted,
                    &separator(shell, *joined),
                );
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
            WordPart::Arithmetic { expression, quoted } => {
                let value = aborting_arithmetic(shell, expression)?.to_string();
                if *quoted {
                    fields.quoted(&value);
                } else {
                    fields.split(&value);
                }
            }
        }
    }
    Ok(())
}

pub fn brace_expand(word: &Word) -> Vec<Word> {
    brace::expand(word)
        .ok()
        .flatten()
        .unwrap_or_else(|| vec![word.clone()])
}

fn separator(shell: &Shell, joined: bool) -> Option<String> {
    joined.then(|| shell.join_separator())
}

fn byte_locale(shell: &Shell) -> bool {
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| shell.get_variable(name).filter(|value| !value.is_empty()));
    matches!(locale.as_deref(), Some("C" | "POSIX"))
}

fn text_length(shell: &Shell, text: &str) -> usize {
    if byte_locale(shell) {
        text.len()
    } else {
        text.chars().count()
    }
}

fn length(shell: &mut Shell, name: &str, subscript: Option<&Subscript>) -> Result<usize, String> {
    let all = matches!(subscript, Some(Subscript::All | Subscript::Joined));
    if subscript.is_none() && (name == "@" || name == "*") {
        return Ok(shell.positional.len());
    }
    if all {
        if let Some(values) = shell.special_array(name) {
            return Ok(values.len());
        }
        let name = shell.resolve_name(name);
        return Ok(match shell.arrays.get(&name) {
            Some(array) => array.len(),
            None => usize::from(shell.get_variable(&name).is_some()),
        });
    }
    Ok(match lookup(shell, name, subscript)? {
        Expanded::Missing if shell.options.nounset => {
            return Err(format!("{name}: unbound variable"));
        }
        Expanded::Missing => 0,
        Expanded::One(text) => text_length(shell, &text),
        Expanded::Many(values) => values.first().map_or(0, |text| text_length(shell, text)),
    })
}

pub fn assignment_tildes(word: &Word) -> Cow<'_, Word> {
    split_tildes(word, 0)
}

fn argument_tildes(word: &Word) -> Cow<'_, Word> {
    let Some(WordPart::Literal(first)) = word.parts.first() else {
        return Cow::Borrowed(word);
    };
    if !word
        .parts
        .iter()
        .any(|part| matches!(part, WordPart::Literal(text) if text.contains('~')))
    {
        return Cow::Borrowed(word);
    }
    let name_end = first
        .find(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))
        .unwrap_or(first.len());
    if !crate::syntax::parser::is_name(&first[..name_end]) {
        return Cow::Borrowed(word);
    }
    let rest = &first[name_end..];
    let rest = match rest.strip_prefix('[') {
        Some(inside) => match inside.find(']') {
            Some(close) => &inside[close + 1..],
            None => return Cow::Borrowed(word),
        },
        None => rest,
    };
    let rest = rest.strip_prefix('+').unwrap_or(rest);
    if !rest.starts_with('=') {
        return Cow::Borrowed(word);
    }
    split_tildes(word, first.len() - rest.len() + 1)
}

fn is_tilde_user(user: &str) -> bool {
    matches!(user, "" | "+" | "-")
        || user
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

fn fallback_tildes(part: &WordPart) -> Option<WordPart> {
    let WordPart::Parameter {
        name,
        subscript,
        operator: Some(operator),
        quoted: false,
    } = part
    else {
        return None;
    };
    let ParameterOperation::Fallback { kind, colon, word } = operator.as_ref() else {
        return None;
    };
    let Cow::Owned(word) = split_tildes(word, 0) else {
        return None;
    };
    Some(WordPart::Parameter {
        name: name.clone(),
        subscript: subscript.clone(),
        operator: Some(Box::new(ParameterOperation::Fallback {
            kind: *kind,
            colon: *colon,
            word,
        })),
        quoted: false,
    })
}

fn split_tildes(word: &Word, value_start: usize) -> Cow<'_, Word> {
    let mut parts = Vec::with_capacity(word.parts.len());
    let mut changed = false;
    for (part_index, part) in word.parts.iter().enumerate() {
        let WordPart::Literal(text) = part else {
            let nested = fallback_tildes(part);
            changed |= nested.is_some();
            parts.push(nested.unwrap_or_else(|| part.clone()));
            continue;
        };
        let start = if part_index == 0 { value_start } else { 0 };
        let mut copied = 0;
        for (index, character) in text.char_indices() {
            let at_boundary = (part_index == 0 && index == value_start)
                || (index > start && text[..index].ends_with(':'));
            if character != '~' || index < copied || !at_boundary {
                continue;
            }
            let rest = &text[index + 1..];
            let end = rest.find(['/', ':']).unwrap_or(rest.len());
            let user = &rest[..end];
            if !is_tilde_user(user) {
                continue;
            }
            if index > copied {
                parts.push(WordPart::Literal(text[copied..index].to_string()));
            }
            parts.push(WordPart::Tilde(user.to_string()));
            copied = index + 1 + end;
            changed = true;
        }
        if copied < text.len() {
            parts.push(WordPart::Literal(text[copied..].to_string()));
        }
    }
    if changed {
        Cow::Owned(Word { parts })
    } else {
        Cow::Borrowed(word)
    }
}

pub fn evaluate_arithmetic(shell: &mut Shell, text: &str) -> Result<i64, String> {
    arith::eval(shell, text)
}

pub fn arithmetic(shell: &mut Shell, expression: &Word) -> Result<i64, String> {
    let text = single(shell, expression)?;
    arith::eval(shell, &text)
}

fn aborting_arithmetic(shell: &mut Shell, expression: &Word) -> Result<i64, String> {
    let text = single(shell, expression)?;
    arith::eval(shell, &text).inspect_err(|_| shell.flow = crate::shell::Flow::Abort)
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

fn push_value(fields: &mut Fields, value: Expanded, quoted: bool, separator: &Option<String>) {
    match value {
        Expanded::Missing if quoted && separator.is_none() => fields.quoted(""),
        Expanded::Missing => {}
        Expanded::One(text) if quoted => fields.quoted(&text),
        Expanded::One(text) => fields.split(&text),
        Expanded::Many(values) if quoted || !fields.split => {
            let joined = match separator {
                Some(separator) => Some(values.join(separator)),
                None if !fields.split => Some(values.join(" ")),
                None => None,
            };
            match joined {
                Some(text) if quoted => fields.quoted(&text),
                Some(text) => fields.split(&text),
                None => {
                    for (index, value) in values.iter().enumerate() {
                        if index > 0 {
                            fields.break_field();
                        }
                        fields.quoted(value);
                    }
                }
            }
        }
        Expanded::Many(values) if fields.ifs.is_empty() => {
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    fields.break_field();
                }
                fields.split(value);
            }
        }
        Expanded::Many(values) => {
            let joiner = fields.ifs.chars().next().map(String::from);
            fields.split(&values.join(joiner.as_deref().unwrap_or(" ")));
        }
    }
}

fn lookup(
    shell: &mut Shell,
    name: &str,
    subscript: Option<&Subscript>,
) -> Result<Expanded, String> {
    let positional = name == "@" || name == "*";
    let resolved;
    let name = if shell.is_reference(name) {
        resolved = shell.resolve_name(name);
        match (crate::shell::split_subscript(&resolved), subscript) {
            (Some(_), None) => {
                return Ok(shell
                    .get_variable(name)
                    .map_or(Expanded::Missing, Expanded::One));
            }
            (Some((base, _)), Some(_)) => base,
            (None, _) => resolved.as_str(),
        }
    } else {
        name
    };
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

struct Parameter<'a> {
    name: &'a str,
    subscript: Option<&'a Subscript>,
    quoted: bool,
}

impl Parameter<'_> {
    fn joined(&self) -> bool {
        self.name == "*" || matches!(self.subscript, Some(Subscript::Joined))
    }

    fn is_special(&self) -> bool {
        matches!(self.name, "@" | "*") && self.subscript.is_none()
    }
}

fn discarded(shell: &mut Shell, message: String) -> String {
    shell.flow = crate::shell::Flow::Abort;
    message
}

fn expand_parameter(
    shell: &mut Shell,
    fields: &mut Fields,
    parameter: &Parameter,
    value: Expanded,
    operator: Option<&ParameterOperation>,
) -> Result<(), String> {
    if let Some(ParameterOperation::Fallback { kind, colon, word }) = operator {
        let unset = fallback_applies(shell, parameter, &value, *colon);
        let chosen = match kind {
            FallbackKind::Default => unset,
            FallbackKind::Alternative => !unset,
            _ => false,
        };
        if chosen {
            return expand_fallback_word(shell, word, parameter.quoted, fields);
        }
    } else if value == Expanded::Missing && shell.options.nounset && !parameter.is_special() {
        return Err(format!("{}: unbound variable", parameter.name));
    }
    let value = match operator {
        Some(operator) => apply(shell, parameter, value, operator)?,
        None => value,
    };
    let separator = separator(shell, parameter.joined());
    push_value(fields, value, parameter.quoted, &separator);
    Ok(())
}

fn fallback_applies(shell: &Shell, parameter: &Parameter, value: &Expanded, colon: bool) -> bool {
    match value {
        Expanded::Missing => true,
        Expanded::Many(values) if values.is_empty() => true,
        Expanded::Many(values) if colon => {
            let separator = if parameter.joined() && parameter.quoted {
                shell.join_separator()
            } else {
                " ".into()
            };
            values.join(&separator).is_empty()
        }
        other => colon && other.is_empty_or_missing(),
    }
}

fn expand_fallback_word(
    shell: &mut Shell,
    word: &Word,
    quoted: bool,
    fields: &mut Fields,
) -> Result<(), String> {
    if quoted {
        fields.quoted("");
        return expand_into(shell, word, fields);
    }
    let word = leading_tilde(word);
    let saved = std::mem::replace(&mut fields.split_literals, true);
    let result = expand_into(shell, &word, fields);
    fields.split_literals = saved;
    result
}

fn fallback_text(shell: &mut Shell, word: &Word, quoted: bool) -> Result<String, String> {
    if quoted {
        return single(shell, word);
    }
    single(shell, &leading_tilde(word))
}

pub(crate) fn leading_tilde(word: &Word) -> Cow<'_, Word> {
    let Some(WordPart::Literal(first)) = word.parts.first() else {
        return Cow::Borrowed(word);
    };
    let Some(rest) = first.strip_prefix('~') else {
        return Cow::Borrowed(word);
    };
    let end = rest.find('/').unwrap_or(rest.len());
    let user = &rest[..end];
    if (end == rest.len() && word.parts.len() > 1) || !is_tilde_user(user) {
        return Cow::Borrowed(word);
    }
    let mut parts = vec![WordPart::Tilde(user.to_string())];
    if end < rest.len() {
        parts.push(WordPart::Literal(rest[end..].to_string()));
    }
    parts.extend(word.parts[1..].iter().cloned());
    Cow::Owned(Word { parts })
}

fn indirect_value(
    shell: &mut Shell,
    name: &str,
    subscript: Option<&Subscript>,
) -> Result<Option<String>, String> {
    let shown = match subscript {
        None => name.to_string(),
        Some(Subscript::All) => format!("{name}[@]"),
        Some(Subscript::Joined) => format!("{name}[*]"),
        Some(Subscript::Index(word)) => format!("{name}[{}]", single(shell, word)?),
    };
    match lookup(shell, name, subscript)? {
        Expanded::One(text) => Ok(Some(text)),
        Expanded::Many(values) if !values.is_empty() => Ok(Some(values.join(" "))),
        _ if shell.arrays.contains_key(&shell.resolve_name(name)) => Ok(None),
        _ => Err(discarded(
            shell,
            format!("{shown}: invalid indirect expansion"),
        )),
    }
}

fn is_parameter_reference(target: &str) -> bool {
    if crate::syntax::parser::is_name(target)
        || (!target.is_empty() && target.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return true;
    }
    if target.len() == 1 && "@*#?-$!".contains(target) {
        return true;
    }
    match target.split_once('[') {
        Some((name, rest)) => {
            crate::syntax::parser::is_name(name) && rest.len() > 1 && rest.ends_with(']')
        }
        None => false,
    }
}

fn indirect_target(shell: &mut Shell, target: &str) -> Result<(String, Option<Subscript>), String> {
    if !is_parameter_reference(target) {
        return Err(discarded(shell, format!("{target}: invalid variable name")));
    }
    let Some((name, rest)) = target.split_once('[') else {
        return Ok((target.to_string(), None));
    };
    let index = &rest[..rest.len() - 1];
    let subscript = match index {
        "@" => Subscript::All,
        "*" => Subscript::Joined,
        index => match crate::syntax::lexer::parse_subscript_word(index) {
            Some(word) => Subscript::Index(word),
            None => return Err(discarded(shell, format!("{target}: bad substitution"))),
        },
    };
    Ok((name.to_string(), Some(subscript)))
}

fn transform(shell: &Shell, name: &str, letter: char, text: &str) -> String {
    match letter {
        'Q' | 'K' | 'k' => quote_word(text),
        'E' => crate::syntax::lexer::ansi_c(text),
        'P' => prompt::decode(shell, text),
        'A' => {
            let letters = crate::builtins::attribute_letters(shell, name);
            if letters.is_empty() {
                format!("{name}={}", quote_word(text))
            } else {
                format!("declare -{letters} {name}={}", quote_word(text))
            }
        }
        'U' => text.to_uppercase(),
        'L' => text.to_lowercase(),
        'u' => {
            let mut chars = text.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
        _ => crate::builtins::attribute_letters(shell, name),
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
    parameter: &Parameter,
    value: Expanded,
    operator: &ParameterOperation,
) -> Result<Expanded, String> {
    let name = parameter.name;
    let subscript = parameter.subscript;
    match operator {
        ParameterOperation::Fallback { kind, colon, word } => {
            let unset = fallback_applies(shell, parameter, &value, *colon);
            match kind {
                FallbackKind::Default if unset => Ok(Expanded::One(single(shell, word)?)),
                FallbackKind::Assign if unset => {
                    let assigned = fallback_text(shell, word, parameter.quoted)?;
                    assign_fallback(shell, name, subscript, &assigned)?;
                    Ok(Expanded::One(assigned))
                }
                FallbackKind::Alternative if unset && matches!(value, Expanded::Many(_)) => {
                    Ok(Expanded::Many(Vec::new()))
                }
                FallbackKind::Alternative if unset => Ok(Expanded::Missing),
                FallbackKind::Alternative => Ok(Expanded::One(single(shell, word)?)),
                FallbackKind::Error if unset => {
                    let message = fallback_text(shell, word, parameter.quoted)?;
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
            let pattern = self::pattern(shell, &leading_tilde(pattern))?;
            Ok(value.map(|text| glob::remove_prefix(text, &pattern, *longest)))
        }
        ParameterOperation::RemoveSuffix { longest, pattern } => {
            let pattern = self::pattern(shell, &leading_tilde(pattern))?;
            Ok(value.map(|text| glob::remove_suffix(text, &pattern, *longest)))
        }
        ParameterOperation::Replace {
            all,
            anchor,
            pattern,
            replacement,
        } => {
            let pattern = self::pattern(shell, &leading_tilde(pattern))?;
            let replacement = single(shell, &leading_tilde(replacement))?;
            Ok(value.map(|text| glob::replace(text, &pattern, &replacement, *all, *anchor)))
        }
        ParameterOperation::ChangeCase {
            upper,
            toggle,
            all,
            pattern,
        } => {
            let pattern = if pattern.parts.is_empty() {
                None
            } else {
                Some(self::pattern(shell, pattern)?)
            };
            let change = CaseChange {
                upper: *upper,
                toggle: *toggle,
                all: *all,
                pattern,
            };
            Ok(value.map(|text| change.apply(text)))
        }
        ParameterOperation::Transform(letter) => {
            if value == Expanded::Missing {
                return Ok(match letter {
                    'A' | 'a' => described_missing(shell, name, *letter),
                    _ => Expanded::Missing,
                });
            }
            if let Some(expanded) = transform_array(shell, name, subscript, *letter) {
                return Ok(expanded);
            }
            let name = shell.resolve_name(name);
            Ok(value.map(|text| transform(shell, &name, *letter, text)))
        }
        ParameterOperation::Substring { offset, length } => {
            let offset = aborting_arithmetic(shell, offset)?;
            let length = match length {
                Some(word) => Some(aborting_arithmetic(shell, word)?),
                None => None,
            };
            let bytes = byte_locale(shell);
            Ok(match value {
                Expanded::Many(values) if (name == "@" || name == "*") && subscript.is_none() => {
                    let with_name: Vec<String> =
                        std::iter::once(shell.arg0.clone()).chain(values).collect();
                    Expanded::Many(slice(&with_name, offset, length))
                }
                Expanded::Many(values) => Expanded::Many(slice(&values, offset, length)),
                other if bytes => other.map(|text| {
                    String::from_utf8_lossy(&slice(text.as_bytes(), offset, length)).into_owned()
                }),
                other => other.map(|text| {
                    let chars: Vec<char> = text.chars().collect();
                    slice(&chars, offset, length).into_iter().collect()
                }),
            })
        }
    }
}

struct CaseChange {
    upper: bool,
    toggle: bool,
    all: bool,
    pattern: Option<String>,
}

impl CaseChange {
    fn convert(&self, character: char) -> String {
        let upper = if self.toggle {
            character.is_lowercase()
        } else {
            self.upper
        };
        if upper {
            character.to_uppercase().collect()
        } else {
            character.to_lowercase().collect()
        }
    }

    fn selected(&self, character: char) -> bool {
        self.pattern.as_ref().is_none_or(|pattern| {
            let mut buffer = [0u8; 4];
            glob::matches_pattern(pattern, character.encode_utf8(&mut buffer))
        })
    }

    fn apply(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for (index, character) in text.chars().enumerate() {
            if (self.all || index == 0) && self.selected(character) {
                out.push_str(&self.convert(character));
            } else {
                out.push(character);
            }
        }
        out
    }
}

fn described_missing(shell: &Shell, name: &str, letter: char) -> Expanded {
    let name = shell.resolve_name(name);
    let exists = shell.arrays.contains_key(&name) || shell.attributes.contains_key(&name);
    if !exists {
        return Expanded::Missing;
    }
    let letters = crate::builtins::attribute_letters(shell, &name);
    match letter {
        'a' => Expanded::One(letters),
        _ if letters.is_empty() => Expanded::Missing,
        _ => Expanded::One(format!("declare -{letters} {name}")),
    }
}

fn transform_array(
    shell: &Shell,
    name: &str,
    subscript: Option<&Subscript>,
    letter: char,
) -> Option<Expanded> {
    if !matches!(subscript, Some(Subscript::All | Subscript::Joined)) {
        return None;
    }
    let name = shell.resolve_name(name);
    let array = shell.arrays.get(&name)?;
    match letter {
        'A' => {
            let letters = crate::builtins::attribute_letters(shell, &name);
            Some(Expanded::One(format!(
                "declare -{letters} {name}={}",
                array_assignment(array)
            )))
        }
        'K' => {
            let pairs: Vec<String> = array
                .entries()
                .into_iter()
                .map(|(key, value)| {
                    let key = if array.is_associative() {
                        quote_key(&key)
                    } else {
                        key
                    };
                    format!("{key} {}", quote_value(&value))
                })
                .collect();
            let trailing = if array.is_associative() && !pairs.is_empty() {
                " "
            } else {
                ""
            };
            Some(Expanded::One(pairs.join(" ") + trailing))
        }
        'k' => Some(Expanded::Many(
            array
                .entries()
                .into_iter()
                .flat_map(|(key, value)| [key, value])
                .collect(),
        )),
        _ => None,
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
        "" => shell.get_variable("HOME").or_else(sys::current_user_home),
        "+" => shell.get_variable("PWD"),
        "-" => shell.get_variable("OLDPWD"),
        _ => sys::user_home(user),
    };
    found.unwrap_or_else(|| format!("~{user}"))
}

struct Fields {
    split: bool,
    split_literals: bool,
    done: Vec<(String, String, bool)>,
    current: String,
    pattern: String,
    glob: bool,
    started: bool,
    pattern_mode: bool,
    glob_options: glob::GlobOptions,
    nullglob: bool,
    noglob: bool,
    failglob: bool,
    ifs: Rc<str>,
}

pub(crate) const DEFAULT_IFS: &str = " \t\n";
pub(crate) const NO_MATCH: &str = "no match: ";
pub(crate) const INTERRUPTED: &str = "interrupted";

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
            split_literals: false,
            done: Vec::new(),
            current: String::new(),
            pattern: String::new(),
            glob: false,
            started: false,
            pattern_mode: false,
            glob_options: glob::GlobOptions::default(),
            nullglob: false,
            noglob: false,
            failglob: false,
            ifs: DEFAULT_SEPARATORS.with(Rc::clone),
        }
    }

    fn literal(&mut self, text: &str) {
        self.started = true;
        self.current.push_str(text);
        if self.split || self.pattern_mode {
            self.pattern.push_str(text);
        }
        if self.split && (text.contains('[') || glob::has_magic(text)) {
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
            if matches!(
                character,
                '*' | '?' | '[' | ']' | '\\' | '(' | ')' | '|' | '@' | '+' | '!'
            ) {
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
        if !self.split {
            self.quoted(text);
            return;
        }
        if self.ifs.is_empty() {
            if !text.is_empty() {
                self.literal(text);
            }
            return;
        }
        let ifs = Rc::clone(&self.ifs);
        let separator = |character: char| ifs.contains(character);
        let white = |character: char| separator(character) && character.is_whitespace();
        let mut rest = text.trim_start_matches(white);
        let mut delimited = rest.len() != text.len();
        if let Some(hard) = rest
            .chars()
            .next()
            .filter(|&character| separator(character))
        {
            rest = rest[hard.len_utf8()..].trim_start_matches(white);
            self.started = true;
            delimited = true;
        }
        if delimited {
            self.break_field();
        }
        let pieces = split_ifs(rest, &ifs, None);
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

    fn finish(mut self) -> Result<Vec<String>, String> {
        self.break_field();
        let mut out = Vec::new();
        for (text, pattern, glob) in self.done {
            let glob = glob && !self.noglob && glob::has_magic(&pattern);
            match glob
                .then(|| glob::expand(&pattern, &self.glob_options))
                .flatten()
            {
                Some(matches) => out.extend(matches),
                None if glob && glob::interrupted() => return Err(INTERRUPTED.to_string()),
                None if glob && self.nullglob => {}
                None if glob && self.failglob => {
                    return Err(format!("{NO_MATCH}{}", glob::unescape(&pattern)));
                }
                None => out.push(text),
            }
        }
        Ok(out)
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

    #[test]
    fn tildes_after_colons_in_assignment_words() {
        let mut shell = Shell::new();
        shell.variables.insert("HOME".into(), "/home/me".into());
        assert_eq!(
            expand(&mut shell, "a=~/x:~/y b:~/z"),
            ["a=/home/me/x:/home/me/y", "b:~/z"]
        );
    }

    #[test]
    fn star_and_at_in_every_context() {
        let mut shell = Shell::new();
        shell.positional = vec!["a b".into(), "c".into()];
        shell.set_variable("IFS", ",");
        assert_eq!(expand(&mut shell, "\"$*\""), ["a b,c"]);
        assert_eq!(expand(&mut shell, "$*"), ["a b", "c"]);
        let word = lexer::lex("\"$@\"").unwrap().tokens.remove(0);
        let Token::Word(word) = word.token else {
            panic!()
        };
        assert_eq!(single(&mut shell, &word).unwrap(), "a b c");
    }

    #[test]
    fn case_modification_with_patterns() {
        let mut shell = Shell::new();
        shell.set_variable("word", "hello");
        assert_eq!(expand(&mut shell, "${word^^[lo]}"), ["heLLO"]);
        assert_eq!(expand(&mut shell, "${word~}"), ["Hello"]);
    }

    #[test]
    fn length_counts_bytes_in_the_c_locale() {
        let mut shell = Shell::new();
        shell.set_variable("text", "\u{e9}");
        shell.unexport_variable("LC_ALL");
        shell.set_variable("LC_ALL", "en_US.UTF-8");
        assert_eq!(expand(&mut shell, "${#text}"), ["1"]);
        shell.set_variable("LC_ALL", "C");
        assert_eq!(expand(&mut shell, "${#text}"), ["2"]);
    }
}
