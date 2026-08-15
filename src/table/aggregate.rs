use std::cmp::Ordering;

use super::data::Table;
use super::value::Value;

fn single_column<'a>(command: &str, args: &'a [String]) -> Result<&'a str, String> {
    match args {
        [column] => Ok(column),
        _ => Err(format!("usage: {command} <column>")),
    }
}

pub(super) fn group_by(args: &[String], table: &Table) -> Result<Table, String> {
    let column = single_column("group-by", args)?;
    let index = table.column(column)?;
    let mut groups: Vec<(Value, i64)> = Vec::new();
    for row in &table.rows {
        match groups
            .iter_mut()
            .find(|(value, _)| value.compare(&row[index]) == Some(Ordering::Equal))
        {
            Some((_, count)) => *count += 1,
            None => groups.push((row[index].clone(), 1)),
        }
    }
    groups.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.sort_cmp(&right.0)));
    let mut grouped = Table::new(vec![table.columns[index].clone(), "count".into()]);
    for (value, count) in groups {
        grouped.push(vec![value, Value::Int(count)]);
    }
    Ok(grouped)
}

pub(super) fn uniq_by(args: &[String], mut table: Table) -> Result<Table, String> {
    let column = single_column("uniq-by", args)?;
    let index = table.column(column)?;
    let mut seen: Vec<Value> = Vec::new();
    table.rows.retain(|row| {
        let value = &row[index];
        if seen
            .iter()
            .any(|known| known.compare(value) == Some(Ordering::Equal))
        {
            false
        } else {
            seen.push(value.clone());
            true
        }
    });
    Ok(table)
}

fn numeric_result(sample: &Value, number: f64) -> Value {
    match sample {
        Value::Size(_) => Value::Size(number.max(0.0).round() as u64),
        Value::Int(_) if number.fract() == 0.0 => Value::Int(number as i64),
        Value::Date(_) => Value::Date(number.round() as i64),
        _ => Value::Float(number),
    }
}

pub(super) fn math(args: &[String], table: &Table) -> Result<String, String> {
    let [operation, column] = args else {
        return Err("usage: math sum|avg|min|max|median <column>".into());
    };
    let index = table.column(column)?;
    let values: Vec<&Value> = table
        .rows
        .iter()
        .map(|row| &row[index])
        .filter(|value| **value != Value::Nothing)
        .collect();
    let Some(sample) = values.first().copied() else {
        return Ok("\n".into());
    };
    let numbers: Vec<f64> = values
        .iter()
        .map(|value| {
            value
                .as_number()
                .ok_or_else(|| format!("'{}' in column '{column}' is not a number", value.raw()))
        })
        .collect::<Result<_, _>>()?;
    let result = match operation.as_str() {
        "sum" => numeric_result(sample, numbers.iter().sum()),
        "avg" | "mean" => {
            numeric_result(sample, numbers.iter().sum::<f64>() / numbers.len() as f64)
        }
        "min" => numeric_result(
            sample,
            numbers.iter().copied().fold(f64::INFINITY, f64::min),
        ),
        "max" => numeric_result(
            sample,
            numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ),
        "median" => {
            let mut sorted = numbers.clone();
            sorted.sort_by(f64::total_cmp);
            let middle = sorted.len() / 2;
            let median = if sorted.len().is_multiple_of(2) {
                (sorted[middle - 1] + sorted[middle]) / 2.0
            } else {
                sorted[middle]
            };
            numeric_result(sample, median)
        }
        other => return Err(format!("unknown operation '{other}'")),
    };
    Ok(result.plain() + "\n")
}

pub(super) fn update(args: &[String], mut table: Table) -> Result<Table, String> {
    let [column, value @ ..] = args else {
        return Err("usage: update <column> <value>".into());
    };
    if value.is_empty() {
        return Err("usage: update <column> <value>".into());
    }
    let text = value.join(" ");
    let index = match table.column(column) {
        Ok(index) => index,
        Err(_) => {
            table.columns.push(column.clone());
            for row in &mut table.rows {
                row.push(Value::Nothing);
            }
            table.columns.len() - 1
        }
    };
    let hint = table
        .rows
        .iter()
        .map(|row| &row[index])
        .find(|value| **value != Value::Nothing)
        .cloned()
        .unwrap_or(Value::Text(String::new()));
    let replacement = match hint {
        Value::Text(_) => Value::infer(&text),
        other => other.literal_like(&text),
    };
    for row in &mut table.rows {
        row[index] = replacement.clone();
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    fn sample() -> Table {
        let mut table = Table::new(arguments("name kind size"));
        for (name, kind, size) in [("a", "file", 100), ("b", "dir", 4096), ("c", "file", 300)] {
            table.push(vec![
                Value::Text(name.into()),
                Value::Text(kind.into()),
                Value::Size(size),
            ]);
        }
        table
    }

    #[test]
    fn grouping_counts_and_orders_by_size() {
        let grouped = group_by(&arguments("kind"), &sample()).unwrap();
        assert_eq!(grouped.columns, ["kind", "count"]);
        assert_eq!(grouped.rows[0], [Value::Text("file".into()), Value::Int(2)]);
        assert_eq!(grouped.rows[1], [Value::Text("dir".into()), Value::Int(1)]);
    }

    #[test]
    fn uniq_keeps_first_rows() {
        let unique = uniq_by(&arguments("kind"), sample()).unwrap();
        let names: Vec<String> = unique.rows.iter().map(|row| row[0].raw()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn math_keeps_the_column_type() {
        assert_eq!(math(&arguments("sum size"), &sample()).unwrap(), "4.4 KB\n");
        assert_eq!(math(&arguments("max size"), &sample()).unwrap(), "4.0 KB\n");
        assert_eq!(
            math(&arguments("median size"), &sample()).unwrap(),
            "300 B\n"
        );
        assert!(math(&arguments("sum name"), &sample()).is_err());
        assert!(math(&arguments("product size"), &sample()).is_err());
    }

    #[test]
    fn update_sets_or_adds_a_column() {
        let updated = update(&arguments("kind other"), sample()).unwrap();
        assert!(
            updated
                .rows
                .iter()
                .all(|row| row[1] == Value::Text("other".into()))
        );
        let added = update(&arguments("checked true"), sample()).unwrap();
        assert_eq!(added.columns.last().map(String::as_str), Some("checked"));
        assert_eq!(added.rows[0][3], Value::Bool(true));
    }
}
