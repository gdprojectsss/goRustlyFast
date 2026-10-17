use crate::expand;
use crate::shell::Shell;
use crate::syntax::ast::{Word, WordPart};
use crate::syntax::lexer;
use crate::variables::{Array, Element, Key};

pub(crate) struct KeyedWord {
    pub key: Word,
    pub append: bool,
    pub value: Word,
}

pub(crate) fn subscript_close(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, character) {
            (Some('\''), '\'') => quote = None,
            (Some('\''), _) => {}
            (_, '\\') => escaped = true,
            (Some('"'), '"') => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '[') => depth += 1,
            (None, ']') if depth == 0 => return Some(index),
            (None, ']') => depth -= 1,
            _ => {}
        }
    }
    None
}

pub(crate) fn keyed_word(word: &Word) -> Option<KeyedWord> {
    let Some(WordPart::Literal(first)) = word.parts.first() else {
        return None;
    };
    first.strip_prefix('[')?;
    let mut key = Vec::new();
    let mut depth = 0usize;
    for (part_index, part) in word.parts.iter().enumerate() {
        let WordPart::Literal(text) = part else {
            key.push(part.clone());
            continue;
        };
        let skip = usize::from(part_index == 0);
        for (index, character) in text.char_indices().skip(skip) {
            match character {
                '[' => depth += 1,
                ']' if depth > 0 => depth -= 1,
                ']' => {
                    let rest = &text[index + 1..];
                    let (append, value_text) = if let Some(value) = rest.strip_prefix("+=") {
                        (true, value)
                    } else {
                        (false, rest.strip_prefix('=')?)
                    };
                    if index > skip {
                        key.push(WordPart::Literal(text[skip..index].to_string()));
                    }
                    let mut value = Vec::new();
                    if !value_text.is_empty() {
                        value.push(WordPart::Literal(value_text.to_string()));
                    }
                    value.extend(word.parts[part_index + 1..].iter().cloned());
                    return Some(KeyedWord {
                        key: Word { parts: key },
                        append,
                        value: Word { parts: value },
                    });
                }
                _ => {}
            }
        }
        if skip < text.len() {
            key.push(WordPart::Literal(text[skip..].to_string()));
        }
    }
    None
}

impl Shell {
    pub(crate) fn subscript_key(&mut self, name: &str, text: &str) -> Result<Key, String> {
        let name = self.resolve_name(name);
        if self.is_associative(&name) {
            let word = lexer::parse_unbroken_word(text).map_err(|error| error.to_string())?;
            let key = expand::single(self, &word)?;
            if key.is_empty() {
                return Err(format!("{name}[{text}]: bad array subscript"));
            }
            return Ok(Key::Name(key));
        }
        let text = if text.contains(['$', '`']) {
            let word = lexer::parse_double_quoted_text(text).map_err(|error| error.to_string())?;
            expand::single(self, &word)?
        } else {
            text.to_string()
        };
        let index = expand::evaluate_arithmetic(self, &text)
            .inspect_err(|_| self.flow = crate::shell::Flow::Abort)?;
        Ok(Key::Index(index))
    }

    fn element_key(&mut self, name: &str, text: &str) -> Result<Key, String> {
        if self.is_associative(name) {
            if text.is_empty() {
                return Err(format!("{name}: []: bad array subscript"));
            }
            return Ok(Key::Name(text.to_string()));
        }
        let index = expand::evaluate_arithmetic(self, text)
            .inspect_err(|_| self.flow = crate::shell::Flow::Abort)?;
        Ok(Key::Index(index))
    }

    pub(crate) fn evaluate_elements(&mut self, words: &[Word]) -> Result<Vec<Element>, String> {
        let mut elements = Vec::with_capacity(words.len());
        for word in words {
            match keyed_word(word) {
                Some(keyed) => {
                    let subscript = expand::single(self, &keyed.key)?;
                    let value = expand::single(self, &expand::assignment_tildes(&keyed.value))?;
                    elements.push(Element::Keyed {
                        subscript,
                        append: keyed.append,
                        value,
                    });
                }
                None => {
                    for value in expand::words(self, std::slice::from_ref(word))? {
                        elements.push(Element::Plain(value));
                    }
                }
            }
        }
        Ok(elements)
    }

    pub(crate) fn assign_elements(
        &mut self,
        name: &str,
        append: bool,
        elements: Vec<Element>,
    ) -> Result<(), String> {
        let name = self.resolve_name(name);
        if self.is_readonly(&name) {
            return Err(format!("{name}: readonly variable"));
        }
        let associative = self.is_associative(&name);
        let existing = self.arrays.remove(&name);
        let scalar = self.variables.get(&name).cloned().or_else(|| {
            (existing.is_none() && append)
                .then(|| crate::bytes::env_var(&name))
                .flatten()
        });
        let array = match existing {
            Some(existing) if append => existing,
            Some(existing) if existing.is_associative() => Array::associative(),
            _ if append => Array::indexed(scalar.into_iter().collect()),
            _ => Array::indexed(Vec::new()),
        };
        let mut next = array.next_index();
        self.set_array(&name, array)?;
        let pairs = associative
            && !elements.is_empty()
            && matches!(elements.first(), Some(Element::Plain(_)));
        let mut elements = elements.into_iter();
        let mut failure = None;
        while let Some(element) = elements.next() {
            let (key, append_value, value) = match element {
                Element::Keyed {
                    subscript,
                    append,
                    value,
                } => match self.element_key(&name, &subscript) {
                    Ok(key) => (key, append, value),
                    Err(message) => {
                        failure.get_or_insert(message);
                        continue;
                    }
                },
                Element::Plain(key) if pairs => {
                    let value = match elements.next() {
                        Some(Element::Plain(value)) => value,
                        _ => String::new(),
                    };
                    (Key::Name(key), false, value)
                }
                Element::Plain(value) if associative => {
                    failure.get_or_insert(format!(
                        "{name}: {value}: must use a subscript when assigning an associative array"
                    ));
                    continue;
                }
                Element::Plain(value) => (Key::Index(next), false, value),
            };
            let value = if append_value {
                let current = self
                    .arrays
                    .get(&name)
                    .and_then(|array| array.get(&key))
                    .cloned()
                    .unwrap_or_default();
                current + &value
            } else {
                value
            };
            let index = match &key {
                Key::Index(index) if *index < 0 => self
                    .arrays
                    .get(&name)
                    .map_or(*index, |array| array.next_index() + index),
                Key::Index(index) => *index,
                Key::Name(_) => 0,
            };
            match self.set_element(&name, key, &value) {
                Ok(()) => next = index + 1,
                Err(message) => {
                    failure.get_or_insert(message);
                }
            }
        }
        match failure {
            Some(message) => Err(message),
            None => Ok(()),
        }
    }
}
