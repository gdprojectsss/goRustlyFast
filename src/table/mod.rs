mod aggregate;
mod data;
mod datetime;
mod delimited;
mod filters;
mod ignore;
pub(crate) mod json;
mod render;
mod search;
mod sources;
mod status;
mod value;
mod wire;

use std::io::Write;

use crate::shell::{self, Shell};
use crate::syntax::ast::{Command, CommandKind};
use crate::sys;

use data::Table;
use json::Json;
use render::Style;
use value::Value;

const COMMANDS: &[&str] = &[
    "files",
    "procs",
    "search",
    "git-status",
    "ports",
    "load",
    "from",
    "to",
    "where",
    "sort-by",
    "select",
    "reject",
    "first",
    "skip",
    "reverse",
    "count",
    "get",
    "group-by",
    "uniq-by",
    "math",
    "update",
];
const CONSUMERS: &[&str] = &[
    "to", "where", "sort-by", "select", "reject", "first", "skip", "reverse", "count", "get",
    "group-by", "uniq-by", "math", "update",
];

pub(crate) enum Output {
    Table(Table),
    Text(String),
}

pub(crate) fn is_command(name: &str) -> bool {
    COMMANDS.contains(&name)
}

pub(crate) fn names() -> &'static [&'static str] {
    COMMANDS
}

pub(crate) fn consumes_tables(command: &Command) -> bool {
    match &command.kind {
        CommandKind::Simple(simple) => simple
            .words
            .first()
            .and_then(|word| word.as_literal())
            .is_some_and(|name| CONSUMERS.contains(&name)),
        _ => false,
    }
}

pub(crate) fn run(shell: &mut Shell, argv: &[String]) -> i32 {
    let name = argv[0].as_str();
    let args = &argv[1..];
    let result = match name {
        "files" => sources::files(args).map(Output::Table),
        "procs" => sources::procs(args).map(Output::Table),
        "git-status" => status::git_status(args).map(Output::Table),
        "ports" => status::ports(args).map(Output::Table),
        "search" => {
            search::parse(args)
                .and_then(|search| search.run())
                .map(|(hits, files_only)| {
                    if shell.pipe_to_table || sys::isatty(1) {
                        Output::Table(search::to_table(&hits, files_only))
                    } else {
                        Output::Text(search::to_text(&hits, files_only))
                    }
                })
        }
        "load" => sources::load(args).map(Output::Table),
        "from" => read_input(name).and_then(|text| sources::from(args, &text).map(Output::Table)),
        "to" => read_table(name).and_then(|table| sources::to(args, &table).map(Output::Text)),
        "count" => {
            read_table(name).and_then(|table| filters::count(args, &table).map(Output::Text))
        }
        "get" => read_table(name).and_then(|table| filters::get(args, &table).map(Output::Text)),
        "where" => {
            read_table(name).and_then(|table| filters::filter_rows(args, table).map(Output::Table))
        }
        "sort-by" => {
            read_table(name).and_then(|table| filters::sort_by(args, table).map(Output::Table))
        }
        "select" => {
            read_table(name).and_then(|table| filters::select(args, table).map(Output::Table))
        }
        "reject" => {
            read_table(name).and_then(|table| filters::reject(args, table).map(Output::Table))
        }
        "first" => {
            read_table(name).and_then(|table| filters::first(args, table).map(Output::Table))
        }
        "skip" => read_table(name).and_then(|table| filters::skip(args, table).map(Output::Table)),
        "reverse" => {
            read_table(name).and_then(|table| filters::reverse(args, table).map(Output::Table))
        }
        "group-by" => {
            read_table(name).and_then(|table| aggregate::group_by(args, &table).map(Output::Table))
        }
        "uniq-by" => {
            read_table(name).and_then(|table| aggregate::uniq_by(args, table).map(Output::Table))
        }
        "math" => {
            read_table(name).and_then(|table| aggregate::math(args, &table).map(Output::Text))
        }
        "update" => {
            read_table(name).and_then(|table| aggregate::update(args, table).map(Output::Table))
        }
        _ => Err("unknown table command".into()),
    };
    match result {
        Ok(output) => {
            emit(shell, output);
            0
        }
        Err(message) => {
            shell::error(format!("{name}: {message}"));
            1
        }
    }
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

pub(crate) fn command_log(shell: &Shell, contents: &str) -> i32 {
    let mut table = Table::new(vec![
        "when".into(),
        "duration".into(),
        "status".into(),
        "directory".into(),
        "command".into(),
    ]);
    for line in contents.lines() {
        let fields: Vec<&str> = line.splitn(5, '\t').collect();
        let [started, milliseconds, status, directory, command] = fields.as_slice() else {
            continue;
        };
        table.push(vec![
            started.parse().map_or(Value::Nothing, Value::Date),
            milliseconds
                .parse::<f64>()
                .map_or(Value::Nothing, |milliseconds| {
                    Value::Float(milliseconds / 1000.0)
                }),
            status.parse().map_or(Value::Nothing, Value::Int),
            Value::Text(unescape(directory)),
            Value::Text(unescape(command)),
        ]);
    }
    emit(shell, Output::Table(table));
    0
}

pub(crate) fn parse_untrusted(text: &str) {
    let _ = parse_auto(text);
    let _ = json::parse(text);
    let _ = delimited::parse(text, ',');
    let _ = wire::decode(text);
    let _ = Table::from_whitespace(text);
}

fn emit(shell: &Shell, output: Output) {
    let text = match output {
        Output::Text(text) => text,
        Output::Table(table) if shell.pipe_to_table => wire::encode(&table),
        Output::Table(table) if sys::isatty(1) => crate::text::without_colors(render::render(
            &table,
            Style::Pretty,
            sys::terminal_width(1),
            datetime::now(),
        )),
        Output::Table(table) => render::render(&table, Style::Plain, None, datetime::now()),
    };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn read_input(command: &str) -> Result<String, String> {
    if sys::isatty(0) {
        return Err(format!(
            "expects input from a pipe, for example: files | {command} ..."
        ));
    }
    Ok(String::from_utf8_lossy(&sys::read_all(0)).into_owned())
}

fn read_table(command: &str) -> Result<Table, String> {
    read_input(command).and_then(|text| parse_auto(&text))
}

fn parse_auto(text: &str) -> Result<Table, String> {
    if let Some(decoded) = wire::decode(text) {
        return decoded;
    }
    let trimmed = text.trim_start();
    if trimmed.starts_with(['[', '{']) {
        if let Ok(parsed) = json::parse(text) {
            return Ok(Table::from_json(&parsed));
        }
        if let Ok(lines) = json::parse_lines(text) {
            return Ok(Table::from_json(&Json::Array(lines)));
        }
    }
    Ok(Table::from_whitespace(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_detection() {
        let from_json = parse_auto("[{\"a\": 1}]").unwrap();
        assert_eq!(from_json.columns, ["a"]);
        let from_lines = parse_auto("{\"a\": 1}\n{\"a\": 2}\n").unwrap();
        assert_eq!(from_lines.rows.len(), 2);
        let from_text = parse_auto("NAME SIZE\nx 1\n").unwrap();
        assert_eq!(from_text.columns, ["NAME", "SIZE"]);
    }
}
