use super::data::Table;
use super::json::{self, Json};
use super::value::Value;

const HEADER: &str = "#grfsh-table v1";

pub(crate) fn encode(table: &Table) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    Json::Array(table.columns.iter().cloned().map(Json::String).collect()).write(&mut out);
    out.push('\n');
    for row in &table.rows {
        Json::Array(row.iter().map(encode_value).collect()).write(&mut out);
        out.push('\n');
    }
    out
}

fn encode_value(value: &Value) -> Json {
    match value {
        Value::Size(bytes) => Json::Object(vec![("size".into(), Json::Number(bytes.to_string()))]),
        Value::Date(timestamp) => {
            Json::Object(vec![("date".into(), Json::Number(timestamp.to_string()))])
        }
        Value::Text(text) => Json::String(text.clone()),
        other => other.to_json(),
    }
}

fn decode_value(json: &Json) -> Value {
    match json {
        Json::Object(fields) if fields.len() == 1 => {
            let (key, inner) = &fields[0];
            let number = match inner {
                Json::Number(text) => text.parse::<i64>().ok(),
                _ => None,
            };
            match (key.as_str(), number) {
                ("size", Some(bytes)) => Value::Size(bytes.max(0) as u64),
                ("date", Some(timestamp)) => Value::Date(timestamp),
                _ => Value::from_json(json),
            }
        }
        Json::Number(text) if text.contains(['.', 'e', 'E']) => {
            text.parse().map_or(Value::Nothing, Value::Float)
        }
        other => Value::from_json(other),
    }
}

pub(crate) fn decode(text: &str) -> Option<Result<Table, String>> {
    let body = text.strip_prefix(HEADER)?.strip_prefix('\n')?;
    Some(decode_body(body))
}

fn decode_body(body: &str) -> Result<Table, String> {
    let mut lines = body.lines();
    let columns = match lines.next().map(json::parse).transpose()? {
        Some(Json::Array(names)) => names
            .into_iter()
            .map(|name| match name {
                Json::String(text) => Ok(text),
                _ => Err("table header must list column names".to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("table header missing".into()),
    };
    let mut table = Table::new(columns);
    for line in lines {
        match json::parse(line)? {
            Json::Array(cells) => table.push(cells.iter().map(decode_value).collect()),
            _ => return Err("table row must be an array".into()),
        }
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_types() {
        let mut table = Table::new(vec![
            "name".into(),
            "size".into(),
            "when".into(),
            "load".into(),
        ]);
        table.push(vec![
            Value::Text("a b".into()),
            Value::Size(2048),
            Value::Date(1_700_000_000),
            Value::Float(2.0),
        ]);
        table.push(vec![Value::Nothing, Value::Int(-3), Value::Bool(true)]);
        let decoded = decode(&encode(&table)).unwrap().unwrap();
        assert_eq!(decoded, table);
    }

    #[test]
    fn plain_text_is_not_a_table() {
        assert!(decode("name size\nx 1\n").is_none());
    }
}
