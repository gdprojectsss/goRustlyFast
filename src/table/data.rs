use super::delimited;
use super::json::Json;
use super::value::Value;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Value {
    pub(crate) fn to_json(&self) -> Json {
        match self {
            Value::Nothing => Json::Null,
            Value::Bool(flag) => Json::Bool(*flag),
            Value::Int(number) => Json::Number(number.to_string()),
            Value::Float(number) if number.is_finite() => Json::Number(format!("{number:?}")),
            Value::Float(_) => Json::Null,
            Value::Size(bytes) => Json::Number(bytes.to_string()),
            Value::Date(_) | Value::Text(_) => Json::String(self.raw()),
        }
    }

    pub(crate) fn from_json(json: &Json) -> Value {
        match json {
            Json::Null => Value::Nothing,
            Json::Bool(flag) => Value::Bool(*flag),
            Json::Number(text) => match text.parse::<i64>() {
                Ok(number) => Value::Int(number),
                Err(_) => text.parse().map_or(Value::Nothing, Value::Float),
            },
            Json::String(text) => Value::Text(text.clone()),
            nested => Value::Text(nested.to_text()),
        }
    }
}

impl Table {
    pub(crate) fn new(columns: Vec<String>) -> Self {
        Table {
            columns,
            rows: Vec::new(),
        }
    }

    pub(crate) fn column(&self, name: &str) -> Result<usize, String> {
        self.columns
            .iter()
            .position(|column| column == name)
            .or_else(|| {
                self.columns
                    .iter()
                    .position(|column| column.eq_ignore_ascii_case(name))
            })
            .ok_or_else(|| format!("no column '{name}' (columns: {})", self.columns.join(", ")))
    }

    pub(crate) fn push(&mut self, mut row: Vec<Value>) {
        row.resize(self.columns.len(), Value::Nothing);
        self.rows.push(row);
    }

    pub(crate) fn from_records(records: Vec<Vec<String>>) -> Table {
        let mut records = records.into_iter();
        let Some(header) = records.next() else {
            return Table::default();
        };
        let mut table = Table::new(unique_names(header));
        for record in records {
            table.push(record.iter().map(|field| Value::infer(field)).collect());
        }
        table
    }

    pub(crate) fn from_whitespace(text: &str) -> Table {
        let mut lines = text.lines().filter(|line| !line.trim().is_empty());
        let Some(header) = lines.next() else {
            return Table::default();
        };
        let columns: Vec<String> = header.split_whitespace().map(String::from).collect();
        let count = columns.len();
        let mut table = Table::new(unique_names(columns));
        for line in lines {
            let mut cells = Vec::with_capacity(count);
            let mut rest = line.trim_start();
            while cells.len() + 1 < count && !rest.is_empty() {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                cells.push(Value::infer(&rest[..end]));
                rest = rest[end..].trim_start();
            }
            if !rest.is_empty() {
                cells.push(Value::infer(rest.trim_end()));
            }
            table.push(cells);
        }
        table
    }

    pub(crate) fn from_lines(text: &str) -> Table {
        let mut table = Table::new(vec!["line".into()]);
        for line in text.lines() {
            table.push(vec![Value::Text(line.to_string())]);
        }
        table
    }

    pub(crate) fn from_json(json: &Json) -> Table {
        let items = match json {
            Json::Array(items) => items.as_slice(),
            single => std::slice::from_ref(single),
        };
        let mut columns: Vec<String> = Vec::new();
        for item in items {
            match item {
                Json::Object(fields) => {
                    for (key, _) in fields {
                        if !columns.contains(key) {
                            columns.push(key.clone());
                        }
                    }
                }
                _ if !columns.iter().any(|column| column == "value") => {
                    columns.push("value".into());
                }
                _ => {}
            }
        }
        let mut table = Table::new(columns);
        for item in items {
            let row = table
                .columns
                .iter()
                .map(|column| match item {
                    Json::Object(fields) => fields
                        .iter()
                        .find(|(key, _)| key == column)
                        .map_or(Value::Nothing, |(_, value)| Value::from_json(value)),
                    scalar if column == "value" => Value::from_json(scalar),
                    _ => Value::Nothing,
                })
                .collect();
            table.rows.push(row);
        }
        table
    }

    pub(crate) fn to_json(&self) -> Json {
        Json::Array(
            self.rows
                .iter()
                .map(|row| {
                    Json::Object(
                        self.columns
                            .iter()
                            .cloned()
                            .zip(row.iter().map(Value::to_json))
                            .collect(),
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn to_delimited(&self, delimiter: char) -> String {
        let mut out = String::new();
        delimited::write_record(&mut out, self.columns.iter().map(String::as_str), delimiter);
        for row in &self.rows {
            let fields: Vec<String> = row.iter().map(Value::raw).collect();
            delimited::write_record(&mut out, fields.iter().map(String::as_str), delimiter);
        }
        out
    }
}

fn unique_names(names: Vec<String>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::with_capacity(names.len());
    for (index, name) in names.into_iter().enumerate() {
        let base = if name.trim().is_empty() {
            format!("column{}", index + 1)
        } else {
            name.trim().to_string()
        };
        let mut candidate = base.clone();
        let mut suffix = 2;
        while seen.contains(&candidate) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        seen.push(candidate);
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::json;

    #[test]
    fn whitespace_tables_keep_the_tail_in_the_last_column() {
        let table = Table::from_whitespace("PID TTY CMD\n  12 ?   /usr/bin/thing --flag x\n");
        assert_eq!(table.columns, ["PID", "TTY", "CMD"]);
        assert_eq!(table.rows[0][0], Value::Int(12));
        assert_eq!(
            table.rows[0][2],
            Value::Text("/usr/bin/thing --flag x".into())
        );
    }

    #[test]
    fn json_objects_union_their_keys() {
        let parsed = json::parse(r#"[{"a":1},{"b":"x","a":2.5}]"#).unwrap();
        let table = Table::from_json(&parsed);
        assert_eq!(table.columns, ["a", "b"]);
        assert_eq!(table.rows[0], [Value::Int(1), Value::Nothing]);
        assert_eq!(table.rows[1], [Value::Float(2.5), Value::Text("x".into())]);
    }

    #[test]
    fn json_round_trip() {
        let parsed = json::parse(r#"[{"name":"a","n":3}]"#).unwrap();
        assert_eq!(Table::from_json(&parsed).to_json(), parsed);
    }

    #[test]
    fn records_get_unique_column_names() {
        let records = vec![
            vec!["id".into(), "id".into(), "".into()],
            vec!["1".into(), "2".into(), "3".into()],
        ];
        let table = Table::from_records(records);
        assert_eq!(table.columns, ["id", "id_2", "column3"]);
    }

    #[test]
    fn missing_column_errors_list_the_available_ones() {
        let table = Table::new(vec!["name".into(), "size".into()]);
        assert_eq!(table.column("SIZE"), Ok(1));
        assert_eq!(
            table.column("owner").unwrap_err(),
            "no column 'owner' (columns: name, size)"
        );
    }
}
