use std::cmp::Ordering;

use super::data::Table;
use super::value::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Contains,
    NotContains,
}

fn comparison(operator: &str) -> Option<Comparison> {
    Some(match operator {
        "==" | "=" | "eq" | "-eq" => Comparison::Equal,
        "!=" | "ne" | "-ne" => Comparison::NotEqual,
        "<" | "lt" | "-lt" => Comparison::Less,
        "<=" | "le" | "-le" => Comparison::LessOrEqual,
        ">" | "gt" | "-gt" => Comparison::Greater,
        ">=" | "ge" | "-ge" => Comparison::GreaterOrEqual,
        "=~" | "has" | "contains" => Comparison::Contains,
        "!~" | "lacks" => Comparison::NotContains,
        _ => return None,
    })
}

fn holds(comparison: Comparison, cell: &Value, expected: &Value, needle: &str) -> bool {
    let contains = || cell.raw().to_lowercase().contains(&needle.to_lowercase());
    match comparison {
        Comparison::Contains => contains(),
        Comparison::NotContains => !contains(),
        Comparison::Equal => cell.compare(expected) == Some(Ordering::Equal),
        Comparison::NotEqual => cell.compare(expected) != Some(Ordering::Equal),
        Comparison::Less => cell.compare(expected) == Some(Ordering::Less),
        Comparison::LessOrEqual => {
            matches!(
                cell.compare(expected),
                Some(Ordering::Less | Ordering::Equal)
            )
        }
        Comparison::Greater => cell.compare(expected) == Some(Ordering::Greater),
        Comparison::GreaterOrEqual => {
            matches!(
                cell.compare(expected),
                Some(Ordering::Greater | Ordering::Equal)
            )
        }
    }
}

struct Clause {
    index: usize,
    comparison: Comparison,
    expected: Value,
    needle: String,
}

fn clause(table: &Table, words: &[String]) -> Result<Clause, String> {
    let [column, operator, rest @ ..] = words else {
        return Err("usage: where <column> <operator> <value> [and|or ...]".into());
    };
    if rest.is_empty() {
        return Err("usage: where <column> <operator> <value> [and|or ...]".into());
    }
    let comparison = comparison(operator).ok_or_else(|| {
        format!("unknown operator '{operator}' (use == != < <= > >= =~ !~ or eq ne lt le gt ge)")
    })?;
    let index = table.column(column)?;
    let needle = rest.join(" ");
    let hint = table
        .rows
        .iter()
        .map(|row| &row[index])
        .find(|value| **value != Value::Nothing)
        .cloned()
        .unwrap_or(Value::Text(String::new()));
    Ok(Clause {
        index,
        comparison,
        expected: hint.literal_like(&needle),
        needle,
    })
}

pub(super) fn filter_rows(args: &[String], mut table: Table) -> Result<Table, String> {
    let words: Vec<String> = match args {
        [single] => single.split_whitespace().map(String::from).collect(),
        many => many.to_vec(),
    };
    let mut alternatives: Vec<Vec<Clause>> = vec![Vec::new()];
    let mut pending: Vec<String> = Vec::new();
    for word in words.into_iter().chain(std::iter::once("and".to_string())) {
        if word != "and" && word != "or" {
            pending.push(word);
            continue;
        }
        let parsed = clause(&table, &std::mem::take(&mut pending))?;
        if let Some(current) = alternatives.last_mut() {
            current.push(parsed);
        }
        if word == "or" {
            alternatives.push(Vec::new());
        }
    }
    table.rows.retain(|row| {
        alternatives.iter().any(|clauses| {
            clauses.iter().all(|clause| {
                holds(
                    clause.comparison,
                    &row[clause.index],
                    &clause.expected,
                    &clause.needle,
                )
            })
        })
    });
    Ok(table)
}

pub(super) fn sort_by(args: &[String], mut table: Table) -> Result<Table, String> {
    let mut reverse = false;
    let mut keys = Vec::new();
    for argument in args {
        match argument.as_str() {
            "-r" | "--reverse" => reverse = true,
            name => {
                for part in name.split(',').filter(|part| !part.is_empty()) {
                    keys.push(table.column(part)?);
                }
            }
        }
    }
    if keys.is_empty() {
        return Err("usage: sort-by <column>... [-r]".into());
    }
    table.rows.sort_by(|left, right| {
        keys.iter()
            .map(|&key| left[key].sort_cmp(&right[key]))
            .find(|ordering| *ordering != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    });
    if reverse {
        table.rows.reverse();
    }
    Ok(table)
}

fn column_list(args: &[String], table: &Table) -> Result<Vec<usize>, String> {
    let mut indexes = Vec::new();
    for argument in args {
        for part in argument.split(',').filter(|part| !part.is_empty()) {
            indexes.push(table.column(part)?);
        }
    }
    Ok(indexes)
}

pub(super) fn select(args: &[String], table: Table) -> Result<Table, String> {
    let indexes = column_list(args, &table)?;
    if indexes.is_empty() {
        return Err("usage: select <column>...".into());
    }
    Ok(Table {
        columns: indexes
            .iter()
            .map(|&index| table.columns[index].clone())
            .collect(),
        rows: table
            .rows
            .into_iter()
            .map(|row| indexes.iter().map(|&index| row[index].clone()).collect())
            .collect(),
    })
}

pub(super) fn reject(args: &[String], table: Table) -> Result<Table, String> {
    let dropped = column_list(args, &table)?;
    let kept: Vec<String> = table
        .columns
        .iter()
        .enumerate()
        .filter(|(index, _)| !dropped.contains(index))
        .map(|(_, name)| name.clone())
        .collect();
    select(&kept, table)
}

fn count_argument(args: &[String], default: usize) -> Result<usize, String> {
    match args {
        [] => Ok(default),
        [count] => count
            .parse()
            .map_err(|_| format!("'{count}' is not a row count")),
        _ => Err("expected a single row count".into()),
    }
}

pub(super) fn first(args: &[String], mut table: Table) -> Result<Table, String> {
    table.rows.truncate(count_argument(args, 10)?);
    Ok(table)
}

pub(super) fn skip(args: &[String], mut table: Table) -> Result<Table, String> {
    let count = count_argument(args, 1)?.min(table.rows.len());
    table.rows.drain(..count);
    Ok(table)
}

pub(super) fn reverse(args: &[String], mut table: Table) -> Result<Table, String> {
    if let Some(argument) = args.first() {
        return Err(format!("unexpected argument '{argument}'"));
    }
    table.rows.reverse();
    Ok(table)
}

pub(super) fn count(args: &[String], table: &Table) -> Result<String, String> {
    if let Some(argument) = args.first() {
        return Err(format!("unexpected argument '{argument}'"));
    }
    Ok(format!("{}\n", table.rows.len()))
}

pub(super) fn get(args: &[String], table: &Table) -> Result<String, String> {
    let [column] = args else {
        return Err("usage: get <column>".into());
    };
    let index = table.column(column)?;
    Ok(table
        .rows
        .iter()
        .map(|row| row[index].raw() + "\n")
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    fn sample() -> Table {
        let mut table = Table::new(arguments("name size kind"));
        table.push(vec![
            Value::Text("big.iso".into()),
            Value::Size(3 << 30),
            Value::Text("file".into()),
        ]);
        table.push(vec![
            Value::Text("notes.md".into()),
            Value::Size(900),
            Value::Text("file".into()),
        ]);
        table.push(vec![
            Value::Text("src".into()),
            Value::Size(4096),
            Value::Text("dir".into()),
        ]);
        table
    }

    fn names(table: &Table) -> Vec<String> {
        table.rows.iter().map(|row| row[0].raw()).collect()
    }

    #[test]
    fn where_understands_sizes() {
        let kept = filter_rows(&arguments("size > 1kb"), sample()).unwrap();
        assert_eq!(names(&kept), ["big.iso", "src"]);
        let quoted = filter_rows(&["size -lt 1kb".to_string()], sample()).unwrap();
        assert_eq!(names(&quoted), ["notes.md"]);
    }

    #[test]
    fn where_text_operators() {
        let dirs = filter_rows(&arguments("kind == dir"), sample()).unwrap();
        assert_eq!(names(&dirs), ["src"]);
        let matching = filter_rows(&arguments("name =~ .MD"), sample()).unwrap();
        assert_eq!(names(&matching), ["notes.md"]);
        let others = filter_rows(&arguments("name !~ o"), sample()).unwrap();
        assert_eq!(names(&others), ["src"]);
    }

    #[test]
    fn where_reports_bad_input() {
        assert!(filter_rows(&arguments("size"), sample()).is_err());
        assert!(filter_rows(&arguments("size ~~ 1"), sample()).is_err());
        assert!(
            filter_rows(&arguments("owner == me"), sample())
                .unwrap_err()
                .contains("columns: name, size, kind")
        );
    }

    #[test]
    fn where_combines_conditions() {
        let both = filter_rows(&arguments("kind == file and size -lt 1kb"), sample()).unwrap();
        assert_eq!(names(&both), ["notes.md"]);
        let either = filter_rows(&arguments("kind == dir or size -gt 1gb"), sample()).unwrap();
        assert_eq!(names(&either), ["big.iso", "src"]);
        assert!(filter_rows(&arguments("kind == dir and"), sample()).is_err());
    }

    #[test]
    fn sorting() {
        let sorted = sort_by(&arguments("size"), sample()).unwrap();
        assert_eq!(names(&sorted), ["notes.md", "src", "big.iso"]);
        let reversed = sort_by(&arguments("kind,name -r"), sample()).unwrap();
        assert_eq!(names(&reversed), ["notes.md", "big.iso", "src"]);
    }

    #[test]
    fn selecting_and_rejecting_columns() {
        let selected = select(&arguments("kind name"), sample()).unwrap();
        assert_eq!(selected.columns, ["kind", "name"]);
        assert_eq!(selected.rows[0][1], Value::Text("big.iso".into()));
        let rejected = reject(&arguments("size"), sample()).unwrap();
        assert_eq!(rejected.columns, ["name", "kind"]);
    }

    #[test]
    fn slicing_rows() {
        assert_eq!(
            names(&first(&arguments("2"), sample()).unwrap()),
            ["big.iso", "notes.md"]
        );
        assert_eq!(names(&skip(&arguments("2"), sample()).unwrap()), ["src"]);
        assert_eq!(names(&reverse(&[], sample()).unwrap())[0], "src");
        assert!(first(&arguments("many"), sample()).is_err());
    }

    #[test]
    fn scalar_outputs() {
        assert_eq!(count(&[], &sample()).unwrap(), "3\n");
        assert_eq!(
            get(&arguments("name"), &sample()).unwrap(),
            "big.iso\nnotes.md\nsrc\n"
        );
    }
}
