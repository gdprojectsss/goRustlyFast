use std::path::Path;
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

use super::data::Table;
use super::json::{self, Json};
use super::render::{self, Style};
use super::value::Value;
use super::{datetime, delimited};

pub(super) fn files(args: &[String]) -> Result<Table, String> {
    let mut show_hidden = false;
    let mut paths = Vec::new();
    for argument in args {
        match argument.as_str() {
            "-a" | "--all" => show_hidden = true,
            flag if flag.starts_with('-') => return Err(format!("unknown flag '{flag}'")),
            path => paths.push(path.to_string()),
        }
    }
    if paths.is_empty() {
        paths.push(".".into());
    }

    let mut table = Table::new(vec![
        "name".into(),
        "type".into(),
        "size".into(),
        "modified".into(),
    ]);
    for path in &paths {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|error| format!("{path}: {error}"))?;
        if !metadata.is_dir() {
            table.push(file_row(path.clone(), &metadata));
            continue;
        }
        let entries = std::fs::read_dir(path).map_err(|error| format!("{path}: {error}"))?;
        let mut rows: Vec<(String, Vec<Value>)> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') && !show_hidden {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                let shown = if paths.len() > 1 {
                    Path::new(path).join(&name).to_string_lossy().into_owned()
                } else {
                    name
                };
                Some((shown.clone(), file_row(shown, &metadata)))
            })
            .collect();
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        for (_, row) in rows {
            table.push(row);
        }
    }
    Ok(table)
}

fn file_row(name: String, metadata: &std::fs::Metadata) -> Vec<Value> {
    let kind = if metadata.is_symlink() {
        "link"
    } else if metadata.is_dir() {
        "dir"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(Value::Nothing, |duration| {
            Value::Date(duration.as_secs() as i64)
        });
    vec![
        Value::Text(name),
        Value::Text(kind.into()),
        Value::Size(metadata.len()),
        modified,
    ]
}

pub(super) fn procs(args: &[String]) -> Result<Table, String> {
    if let Some(argument) = args.first() {
        return Err(format!("unexpected argument '{argument}'"));
    }
    let output = Command::new("ps")
        .args(["-eo", "pid=,ppid=,pcpu=,pmem=,rss=,comm="])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("could not run ps: {error}"))?;
    Ok(parse_ps(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_ps(text: &str) -> Table {
    let mut table = Table::new(vec![
        "pid".into(),
        "ppid".into(),
        "cpu".into(),
        "mem".into(),
        "rss".into(),
        "name".into(),
    ]);
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let numbers: Vec<&str> = fields.by_ref().take(5).collect();
        let command: Vec<&str> = fields.collect();
        if numbers.len() < 5 || command.is_empty() {
            continue;
        }
        let command = command.join(" ");
        let name = Path::new(&command)
            .file_name()
            .map_or(command.clone(), |name| name.to_string_lossy().into_owned());
        table.push(vec![
            Value::infer(numbers[0]),
            Value::infer(numbers[1]),
            numbers[2].parse().map_or(Value::Nothing, Value::Float),
            numbers[3].parse().map_or(Value::Nothing, Value::Float),
            numbers[4]
                .parse::<u64>()
                .map_or(Value::Nothing, |kilobytes| Value::Size(kilobytes * 1024)),
            Value::Text(name),
        ]);
    }
    table
}

pub(super) fn load(args: &[String]) -> Result<Table, String> {
    let [path] = args else {
        return Err("usage: load <file.csv|file.tsv|file.json|file>".into());
    };
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let extension = Path::new(path)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let format = match extension.as_str() {
        "csv" | "tsv" | "json" | "jsonl" | "ndjson" => extension.as_str(),
        _ => "lines",
    };
    parse_format(format, &text)
}

pub(super) fn from(args: &[String], text: &str) -> Result<Table, String> {
    let [format] = args else {
        return Err("usage: from csv|tsv|json|jsonl|ws|lines".into());
    };
    parse_format(format, text)
}

fn parse_format(format: &str, text: &str) -> Result<Table, String> {
    match format {
        "csv" => Ok(Table::from_records(delimited::parse(text, ',')?)),
        "tsv" => Ok(Table::from_records(delimited::parse(text, '\t')?)),
        "json" => match json::parse(text) {
            Ok(parsed) => Ok(Table::from_json(&parsed)),
            Err(error) => json::parse_lines(text)
                .map(|lines| Table::from_json(&Json::Array(lines)))
                .map_err(|_| error),
        },
        "jsonl" | "ndjson" => Ok(Table::from_json(&Json::Array(json::parse_lines(text)?))),
        "ws" => Ok(Table::from_whitespace(text)),
        "lines" => Ok(Table::from_lines(text)),
        other => Err(format!("unknown format '{other}'")),
    }
}

pub(super) fn to(args: &[String], table: &Table) -> Result<String, String> {
    let [format] = args else {
        return Err("usage: to csv|tsv|json|md|table".into());
    };
    match format.as_str() {
        "csv" => Ok(table.to_delimited(',')),
        "tsv" => Ok(table.to_delimited('\t')),
        "json" => Ok(table.to_json().to_text() + "\n"),
        "md" | "markdown" => Ok(render::markdown(table)),
        "table" | "text" => Ok(render::render(table, Style::Plain, None, datetime::now())),
        other => Err(format!("unknown format '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ps_output_parses_into_typed_columns() {
        let table =
            parse_ps("  101     1   2.5  0.3   2048 /usr/sbin/sshd\n  7 1 0.0 0.0 16 bash -l\n");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0][0], Value::Int(101));
        assert_eq!(table.rows[0][2], Value::Float(2.5));
        assert_eq!(table.rows[0][4], Value::Size(2048 * 1024));
        assert_eq!(table.rows[0][5], Value::Text("sshd".into()));
        assert_eq!(table.rows[1][5], Value::Text("bash -l".into()));
    }

    #[test]
    fn files_lists_this_crate() {
        let table = files(&["src".into()]).unwrap();
        let names: Vec<String> = table.rows.iter().map(|row| row[0].raw()).collect();
        assert!(names.contains(&"lib.rs".to_string()));
        assert!(names.contains(&"table".to_string()));
    }

    #[test]
    fn formats_convert_both_ways() {
        let table = from(&["csv".into()], "name,n\nx,1\n").unwrap();
        assert_eq!(
            to(&["json".into()], &table).unwrap(),
            "[{\"name\":\"x\",\"n\":1}]\n"
        );
        assert_eq!(to(&["tsv".into()], &table).unwrap(), "name\tn\nx\t1\n");
        assert!(from(&["yaml".into()], "").is_err());
    }
}
