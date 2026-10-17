use std::cmp::Ordering;

use super::data::Table;
use super::regex::Regex;
use super::value::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Matches,
    NotMatches,
    Contains,
    NotContains,
}

const USAGE: &str = "usage: where <column> <operator> <value> [and|or ...]";

fn comparison(operator: &str) -> Option<Comparison> {
    Some(match operator {
        "==" | "=" | "eq" | "-eq" => Comparison::Equal,
        "!=" | "ne" | "-ne" => Comparison::NotEqual,
        "<" | "lt" | "-lt" => Comparison::Less,
        "<=" | "le" | "-le" => Comparison::LessOrEqual,
        ">" | "gt" | "-gt" => Comparison::Greater,
        ">=" | "ge" | "-ge" => Comparison::GreaterOrEqual,
        "=~" => Comparison::Matches,
        "!~" => Comparison::NotMatches,
        "has" | "contains" => Comparison::Contains,
        "lacks" => Comparison::NotContains,
        _ => return None,
    })
}

fn holds(clause: &Clause, cell: &Value) -> bool {
    let contains = || {
        cell.raw()
            .to_lowercase()
            .contains(&clause.needle.to_lowercase())
    };
    let matches = || {
        clause
            .regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(&cell.raw()))
    };
    let ordering = || cell.compare(&clause.expected);
    match clause.comparison {
        Comparison::Matches => matches(),
        Comparison::NotMatches => !matches(),
        Comparison::Contains => contains(),
        Comparison::NotContains => !contains(),
        Comparison::Equal => ordering() == Some(Ordering::Equal),
        Comparison::NotEqual => ordering() != Some(Ordering::Equal),
        Comparison::Less => ordering() == Some(Ordering::Less),
        Comparison::LessOrEqual => matches!(ordering(), Some(Ordering::Less | Ordering::Equal)),
        Comparison::Greater => ordering() == Some(Ordering::Greater),
        Comparison::GreaterOrEqual => {
            matches!(ordering(), Some(Ordering::Greater | Ordering::Equal))
        }
    }
}

struct Clause {
    index: usize,
    comparison: Comparison,
    expected: Value,
    needle: String,
    regex: Option<Regex>,
}

fn kind_name(value: &Value) -> &'static str {
    match value {
        Value::Size(_) => "size (such as 512, 10kb or 2mb)",
        Value::Date(_) => "date or age (such as 2024-05-01 or 2d)",
        Value::Bool(_) => "true or false value",
        _ => "number",
    }
}

fn typed_value(table: &Table, index: usize, column: &str, text: &str) -> Result<Value, String> {
    let mut cells = table
        .rows
        .iter()
        .map(|row| &row[index])
        .filter(|value| **value != Value::Nothing);
    let Some(hint) = cells.clone().next() else {
        return Ok(Value::infer(text));
    };
    let expected = hint.literal_like(text);
    let textual = matches!(expected, Value::Text(_));
    if textual && !cells.any(|value| matches!(value, Value::Text(_))) {
        return Err(format!(
            "'{text}' is not a valid {} for column '{column}'",
            kind_name(hint)
        ));
    }
    Ok(expected)
}

fn clause(table: &Table, words: &[String]) -> Result<Clause, String> {
    let [column, operator, rest @ ..] = words else {
        return Err(USAGE.into());
    };
    if rest.is_empty() {
        return Err(USAGE.into());
    }
    let comparison = comparison(operator).ok_or_else(|| {
        format!(
            "unknown operator '{operator}' (use == != < <= > >= =~ !~ has lacks or eq ne lt le gt ge)"
        )
    })?;
    let index = table.column(column)?;
    let needle = rest.join(" ");
    let (expected, regex) = match comparison {
        Comparison::Matches | Comparison::NotMatches => {
            (Value::Nothing, Some(Regex::new(&needle, false)?))
        }
        Comparison::Contains | Comparison::NotContains => (Value::Text(needle.clone()), None),
        _ => (typed_value(table, index, column, &needle)?, None),
    };
    Ok(Clause {
        index,
        comparison,
        expected,
        needle,
        regex,
    })
}

fn split_quoted(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for character in text.chars() {
        match (quote, character) {
            (Some(open), close) if open == close => quote = None,
            (Some(_), other) => word.push(other),
            (None, '\'' | '"') => {
                quote = Some(character);
                started = true;
            }
            (None, space) if space.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, other) => {
                word.push(other);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

fn starts_clause(table: &Table, words: &[String]) -> bool {
    match words {
        [] => true,
        [column, operator, ..] => table.column(column).is_ok() && comparison(operator).is_some(),
        _ => false,
    }
}

pub(super) fn filter_rows(args: &[String], mut table: Table) -> Result<Table, String> {
    let words: Vec<String> = match args {
        [single] => split_quoted(single),
        many => many.to_vec(),
    };
    let mut alternatives: Vec<Vec<Clause>> = vec![Vec::new()];
    let mut pending: Vec<String> = Vec::new();
    for (position, word) in words.iter().enumerate() {
        let connector = (word == "and" || word == "or")
            && pending.len() >= 3
            && starts_clause(&table, &words[position + 1..]);
        if !connector {
            pending.push(word.clone());
            continue;
        }
        if position + 1 == words.len() {
            return Err(USAGE.into());
        }
        let parsed = clause(&table, &std::mem::take(&mut pending))?;
        if let Some(current) = alternatives.last_mut() {
            current.push(parsed);
        }
        if word == "or" {
            alternatives.push(Vec::new());
        }
    }
    let parsed = clause(&table, &pending)?;
    if let Some(current) = alternatives.last_mut() {
        current.push(parsed);
    }
    table.rows.retain(|row| {
        alternatives.iter().any(|clauses| {
            clauses
                .iter()
                .all(|clause| holds(clause, &row[clause.index]))
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
    let (separator, column) = match args {
        [column] if !column.starts_with('-') => ('\n', column),
        [flag, column] | [column, flag] if flag == "-0" || flag == "--null" => ('\0', column),
        _ => return Err("usage: get [-0] <column>".into()),
    };
    let index = table.column(column)?;
    Ok(table
        .rows
        .iter()
        .map(|row| {
            let mut value = row[index].raw();
            value.push(separator);
            value
        })
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
        let matching = filter_rows(&arguments("name =~ \\.md$"), sample()).unwrap();
        assert_eq!(names(&matching), ["notes.md"]);
        let cased = filter_rows(&arguments("name =~ .MD"), sample()).unwrap();
        assert!(cased.rows.is_empty());
        let words = filter_rows(&arguments("name =~ ^\\w+\\.\\w{3}$"), sample()).unwrap();
        assert_eq!(names(&words), ["big.iso"]);
        let others = filter_rows(&arguments("name !~ o"), sample()).unwrap();
        assert_eq!(names(&others), ["src"]);
        let substring = filter_rows(&arguments("name has .MD"), sample()).unwrap();
        assert_eq!(names(&substring), ["notes.md"]);
        assert!(filter_rows(&arguments("name =~ (unclosed"), sample()).is_err());
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
        assert_eq!(
            filter_rows(&arguments("size -gt abc"), sample()).unwrap_err(),
            "'abc' is not a valid size (such as 512, 10kb or 2mb) for column 'size'"
        );
    }

    #[test]
    fn where_keeps_and_inside_values() {
        let mut table = Table::new(arguments("name genre"));
        table.push(vec![
            Value::Text("rock and roll".into()),
            Value::Text("a".into()),
        ]);
        table.push(vec![Value::Text("rock".into()), Value::Text("b".into())]);
        let single = filter_rows(&["name == rock and roll".to_string()], table.clone()).unwrap();
        assert_eq!(names(&single), ["rock and roll"]);
        let quoted = filter_rows(&["name == 'rock and roll'".to_string()], table.clone()).unwrap();
        assert_eq!(names(&quoted), ["rock and roll"]);
        let both = filter_rows(&arguments("name == rock and genre == b"), table).unwrap();
        assert_eq!(names(&both), ["rock"]);
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
        assert_eq!(
            get(&arguments("-0 name"), &sample()).unwrap(),
            "big.iso\0notes.md\0src\0"
        );
        assert!(get(&arguments("-x name"), &sample()).is_err());
    }
}
