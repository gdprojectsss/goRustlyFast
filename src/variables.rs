use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Index(i64),
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Array {
    Indexed(BTreeMap<usize, String>),
    Associative(BTreeMap<String, String>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Attributes {
    pub readonly: bool,
    pub integer: bool,
    pub lower: bool,
    pub upper: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Saved {
    pub scalar: Option<String>,
    pub array: Option<Array>,
    pub attributes: Option<Attributes>,
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
        Array::Associative(BTreeMap::new())
    }

    pub fn is_associative(&self) -> bool {
        matches!(self, Array::Associative(_))
    }

    pub fn values(&self) -> Vec<String> {
        match self {
            Array::Indexed(map) => map.values().cloned().collect(),
            Array::Associative(map) => map.values().cloned().collect(),
        }
    }

    pub fn keys(&self) -> Vec<String> {
        match self {
            Array::Indexed(map) => map.keys().map(|index| index.to_string()).collect(),
            Array::Associative(map) => map.keys().cloned().collect(),
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

fn keyed_element(element: &str) -> Option<(&str, &str)> {
    let inner = element.strip_prefix('[')?;
    let close = inner.find("]=")?;
    Some((&inner[..close], &inner[close + 2..]))
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
        assert_eq!(array.keys(), ["host", "port", "user"]);
        assert_eq!(
            array.get(&Key::Name("port".into())).map(String::as_str),
            Some("5432")
        );
        assert!(array.extend(strings(&["plain"])).is_err());
    }
}
