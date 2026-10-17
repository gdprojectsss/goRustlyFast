use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Index(i64),
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Array {
    Indexed(BTreeMap<usize, String>),
    Associative(Associative),
}

const INITIAL_BUCKETS: usize = 1024;
const GROWTH_FACTOR: usize = 4;
const LOAD_FACTOR: usize = 2;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Associative {
    values: HashMap<String, String>,
    buckets: Vec<Vec<String>>,
}

fn bash_hash(key: &str) -> u32 {
    key.bytes().fold(2166136261u32, |hash, byte| {
        hash.wrapping_mul(16777619) ^ (byte as std::ffi::c_char as i32 as u32)
    })
}

impl Associative {
    fn bucket_of(&self, key: &str) -> usize {
        bash_hash(key) as usize & (self.buckets.len() - 1)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    pub fn insert(&mut self, key: String, value: String) {
        if let Some(existing) = self.values.get_mut(&key) {
            *existing = value;
            return;
        }
        if self.buckets.is_empty() {
            self.buckets = vec![Vec::new(); INITIAL_BUCKETS];
        } else if self.values.len() >= self.buckets.len() * LOAD_FACTOR {
            self.grow();
        }
        let bucket = self.bucket_of(&key);
        self.buckets[bucket].push(key.clone());
        self.values.insert(key, value);
    }

    fn grow(&mut self) {
        let size = self.buckets.len() * GROWTH_FACTOR;
        let old = std::mem::replace(&mut self.buckets, vec![Vec::new(); size]);
        for key in old.into_iter().flat_map(|bucket| bucket.into_iter().rev()) {
            let bucket = self.bucket_of(&key);
            self.buckets[bucket].push(key);
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        let value = self.values.remove(key)?;
        let bucket = self.bucket_of(key);
        self.buckets[bucket].retain(|existing| existing != key);
        Some(value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.buckets
            .iter()
            .flat_map(|bucket| bucket.iter().rev())
            .filter_map(|key| self.values.get_key_value(key))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Attributes {
    pub readonly: bool,
    pub integer: bool,
    pub lower: bool,
    pub upper: bool,
    pub exported: bool,
    pub nameref: bool,
    pub invisible: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Saved {
    pub scalar: Option<String>,
    pub array: Option<Array>,
    pub attributes: Option<Attributes>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub name: String,
    pub source: String,
    pub line: usize,
    pub function: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Evaluated {
    Scalar {
        name: String,
        key: Option<Key>,
        append: bool,
        value: String,
    },
    Array {
        name: String,
        append: bool,
        elements: Vec<String>,
    },
}

impl Evaluated {
    pub fn name(&self) -> &str {
        match self {
            Evaluated::Scalar { name, .. } | Evaluated::Array { name, .. } => name,
        }
    }
}

impl Array {
    pub fn indexed(values: Vec<String>) -> Array {
        Array::Indexed(values.into_iter().enumerate().collect())
    }

    pub fn associative() -> Array {
        Array::Associative(Associative::default())
    }

    pub fn is_associative(&self) -> bool {
        matches!(self, Array::Associative(_))
    }

    pub fn len(&self) -> usize {
        match self {
            Array::Indexed(map) => map.len(),
            Array::Associative(map) => map.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Array::Indexed(map) => map.is_empty(),
            Array::Associative(map) => map.is_empty(),
        }
    }

    pub fn values(&self) -> Vec<String> {
        match self {
            Array::Indexed(map) => map.values().cloned().collect(),
            Array::Associative(map) => map.iter().map(|(_, value)| value.clone()).collect(),
        }
    }

    pub fn keys(&self) -> Vec<String> {
        match self {
            Array::Indexed(map) => map.keys().map(|index| index.to_string()).collect(),
            Array::Associative(map) => map.iter().map(|(key, _)| key.clone()).collect(),
        }
    }

    pub fn entries(&self) -> Vec<(String, String)> {
        match self {
            Array::Indexed(map) => map
                .iter()
                .map(|(index, value)| (index.to_string(), value.clone()))
                .collect(),
            Array::Associative(map) => map
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        }
    }

    fn next_index(map: &BTreeMap<usize, String>) -> usize {
        map.keys().next_back().map_or(0, |last| last + 1)
    }

    fn resolve(map: &BTreeMap<usize, String>, index: i64) -> Option<usize> {
        if index < 0 {
            usize::try_from(Self::next_index(map) as i64 + index).ok()
        } else {
            Some(index as usize)
        }
    }

    pub fn get(&self, key: &Key) -> Option<&String> {
        match (self, key) {
            (Array::Indexed(map), Key::Index(index)) => map.get(&Self::resolve(map, *index)?),
            (Array::Indexed(map), Key::Name(name)) => map.get(&name.parse().ok()?),
            (Array::Associative(map), Key::Name(name)) => map.get(name),
            (Array::Associative(map), Key::Index(index)) => map.get(&index.to_string()),
        }
    }

    pub fn first(&self) -> Option<&String> {
        self.get(&Key::Index(0))
    }

    pub fn set(&mut self, key: Key, value: String) -> Result<(), String> {
        match (self, key) {
            (Array::Indexed(map), Key::Index(index)) => {
                let position = Self::resolve(map, index)
                    .ok_or_else(|| format!("{index}: bad array subscript"))?;
                map.insert(position, value);
            }
            (Array::Indexed(map), Key::Name(name)) => {
                let position = name
                    .parse()
                    .map_err(|_| format!("{name}: bad array subscript"))?;
                map.insert(position, value);
            }
            (Array::Associative(map), Key::Name(name)) => {
                map.insert(name, value);
            }
            (Array::Associative(map), Key::Index(index)) => {
                map.insert(index.to_string(), value);
            }
        }
        Ok(())
    }

    pub fn remove(&mut self, key: &Key) {
        match (self, key) {
            (Array::Indexed(map), Key::Index(index)) => {
                if let Some(position) = Self::resolve(map, *index) {
                    map.remove(&position);
                }
            }
            (Array::Associative(map), Key::Name(name)) => {
                map.remove(name);
            }
            (Array::Indexed(map), Key::Name(name)) => {
                if let Ok(position) = name.parse::<usize>() {
                    map.remove(&position);
                }
            }
            (Array::Associative(map), Key::Index(index)) => {
                map.remove(&index.to_string());
            }
        }
    }

    pub fn extend(&mut self, elements: Vec<String>) -> Result<(), String> {
        for element in elements {
            match self {
                Array::Indexed(map) => {
                    if let Some((key, value)) = keyed_element(&element) {
                        let position = key
                            .parse()
                            .map_err(|_| format!("{key}: bad array subscript"))?;
                        map.insert(position, value.to_string());
                    } else {
                        let next = Self::next_index(map);
                        map.insert(next, element);
                    }
                }
                Array::Associative(map) => match keyed_element(&element) {
                    Some(("", _)) => return Err("[]: bad array subscript".into()),
                    Some((key, value)) => {
                        map.insert(key.to_string(), value.to_string());
                    }
                    None => {
                        return Err(format!(
                            "{element}: must use a subscript when assigning an associative array"
                        ));
                    }
                },
            }
        }
        Ok(())
    }
}

pub fn keyed_element(element: &str) -> Option<(&str, &str)> {
    let inner = element.strip_prefix('[')?;
    let close = inner.find("]=")?;
    Some((&inner[..close], &inner[close + 2..]))
}

pub fn needs_ansi_c(text: &str) -> bool {
    text.chars().any(char::is_control)
}

pub fn ansi_c_quote(text: &str) -> String {
    let mut out = String::from("$'");
    for character in text.chars() {
        match character {
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            '\x1b' => out.push_str("\\E"),
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            control if control.is_control() => {
                let mut buffer = [0u8; 4];
                for byte in control.encode_utf8(&mut buffer).bytes() {
                    out.push_str(&format!("\\{byte:03o}"));
                }
            }
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

pub fn double_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        if matches!(character, '"' | '\\' | '$' | '`') {
            out.push('\\');
        }
        out.push(character);
    }
    out.push('"');
    out
}

pub fn single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub fn quote_value(text: &str) -> String {
    if needs_ansi_c(text) {
        ansi_c_quote(text)
    } else {
        double_quote(text)
    }
}

pub fn quote_word(text: &str) -> String {
    if needs_ansi_c(text) {
        ansi_c_quote(text)
    } else {
        single_quote(text)
    }
}

pub fn quote_key(text: &str) -> String {
    if needs_ansi_c(text) {
        ansi_c_quote(text)
    } else if has_shell_metas(text) {
        double_quote(text)
    } else {
        text.to_string()
    }
}

fn has_shell_metas(text: &str) -> bool {
    let mut previous = None;
    for (index, character) in text.chars().enumerate() {
        let meta = match character {
            ' ' | '\t' | '\n' | '\'' | '"' | '\\' | '|' | '&' | ';' | '(' | ')' | '<' | '>'
            | '!' | '{' | '}' | '*' | '[' | '?' | ']' | '^' | '$' | '`' => true,
            '~' => index == 0 || matches!(previous, Some('=' | ':')),
            '#' => index == 0,
            _ => false,
        };
        if meta {
            return true;
        }
        previous = Some(character);
    }
    false
}

pub fn array_assignment(array: &Array) -> String {
    let elements: Vec<String> = array
        .entries()
        .into_iter()
        .map(|(key, value)| {
            let key = if array.is_associative() {
                quote_key(&key)
            } else {
                key
            };
            format!("[{key}]={}", quote_value(&value))
        })
        .collect();
    if array.is_associative() && !elements.is_empty() {
        format!("({} )", elements.join(" "))
    } else {
        format!("({})", elements.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn sparse_indexed_arrays() {
        let mut array = Array::indexed(Vec::new());
        array.extend(strings(&["a", "[5]=f", "g"])).unwrap();
        assert_eq!(array.keys(), ["0", "5", "6"]);
        assert_eq!(array.get(&Key::Index(-1)).map(String::as_str), Some("g"));
        assert_eq!(array.get(&Key::Index(3)), None);
        array.remove(&Key::Index(5));
        assert_eq!(array.values(), ["a", "g"]);
    }

    #[test]
    fn associative_arrays() {
        let mut array = Array::associative();
        array
            .extend(strings(&["[host]=db-1", "[port]=5432"]))
            .unwrap();
        array.set(Key::Name("user".into()), "admin".into()).unwrap();
        assert_eq!(array.keys(), ["port", "host", "user"]);
        assert_eq!(
            array.get(&Key::Name("port".into())).map(String::as_str),
            Some("5432")
        );
        assert!(array.extend(strings(&["plain"])).is_err());
        assert!(array.extend(strings(&["[]=empty"])).is_err());
    }

    #[test]
    fn associative_order_follows_bash_hash_buckets() {
        let mut array = Array::associative();
        for key in ["zeta", "a", "mu", "b", "ab"] {
            array.set(Key::Name(key.into()), "v".into()).unwrap();
        }
        assert_eq!(array.keys(), ["ab", "b", "a", "mu", "zeta"]);
        let mut large = Array::associative();
        for index in 0..2100 {
            large
                .set(Key::Name(format!("k{index}")), String::new())
                .unwrap();
        }
        assert_eq!(large.len(), 2100);
        assert_eq!(large.keys()[..4], ["k1698", "k1699", "k1696", "k1697"]);
        large.remove(&Key::Name("k1698".into()));
        assert_eq!(large.keys()[0], "k1699");
    }

    #[test]
    fn quoting_matches_declare_output() {
        assert_eq!(quote_value("a\"b$c"), "\"a\\\"b\\$c\"");
        assert_eq!(quote_value("tab\there\x1b"), "$'tab\\there\\E'");
        assert_eq!(quote_value("\x01"), "$'\\001'");
        assert_eq!(quote_word("it's"), "'it'\\''s'");
        assert_eq!(quote_key("a=b"), "a=b");
        assert_eq!(quote_key("a b"), "\"a b\"");
        assert_eq!(quote_key("~x"), "\"~x\"");
    }
}
