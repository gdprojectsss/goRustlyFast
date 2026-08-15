use super::data::Table;
use super::value::Value;

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";
const GAP: usize = 2;
const MIN_COLUMN: usize = 6;
const ELLIPSIS: &str = "...";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Style {
    Pretty,
    Plain,
}

fn width_of(text: &str) -> usize {
    text.chars().count()
}

fn fit(text: &str, width: usize) -> String {
    if width_of(text) <= width {
        return text.to_string();
    }
    if width <= ELLIPSIS.len() {
        return text.chars().take(width).collect();
    }
    let kept: String = text.chars().take(width - ELLIPSIS.len()).collect();
    format!("{kept}{ELLIPSIS}")
}

fn pad(out: &mut String, text: &str, width: usize, right_align: bool) {
    let padding = width.saturating_sub(width_of(text));
    if right_align {
        out.extend(std::iter::repeat_n(' ', padding));
        out.push_str(text);
    } else {
        out.push_str(text);
        out.extend(std::iter::repeat_n(' ', padding));
    }
}

fn shrink_to(widths: &mut [usize], limit: usize) {
    let total =
        |widths: &[usize]| widths.iter().sum::<usize>() + GAP * widths.len().saturating_sub(1);
    while total(widths) > limit {
        let Some((index, widest)) = widths
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|(_, width)| *width)
        else {
            return;
        };
        if widest <= MIN_COLUMN {
            return;
        }
        widths[index] -= 1;
    }
}

pub(crate) fn render(table: &Table, style: Style, limit: Option<usize>, now: i64) -> String {
    let cells: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| match style {
                    Style::Pretty => value.display(now),
                    Style::Plain => value.plain(),
                })
                .map(|text| text.replace(['\n', '\t'], " "))
                .collect()
        })
        .collect();

    let right_align: Vec<bool> = (0..table.columns.len())
        .map(|column| {
            let mut values = table
                .rows
                .iter()
                .map(|row| &row[column])
                .filter(|value| **value != Value::Nothing);
            let mut any = false;
            let numeric = values.all(|value| {
                any = true;
                value.is_numeric()
            });
            any && numeric
        })
        .collect();

    let mut widths: Vec<usize> = table.columns.iter().map(|name| width_of(name)).collect();
    for row in &cells {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(width_of(cell));
        }
    }
    if let Some(limit) = limit {
        shrink_to(&mut widths, limit);
    }

    let pretty = style == Style::Pretty;
    let last = table.columns.len().saturating_sub(1);
    let mut out = String::new();

    let mut line = String::new();
    for (index, name) in table.columns.iter().enumerate() {
        if index > 0 {
            line.push_str(&" ".repeat(GAP));
        }
        let text = fit(name, widths[index]);
        let width = if index == last && !right_align[index] {
            0
        } else {
            widths[index]
        };
        pad(&mut line, &text, width, right_align[index]);
    }
    if pretty {
        out.push_str(&format!("{BOLD}{}{RESET}\n", line.trim_end()));
        let rule: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
        out.push_str(&format!("{DIM}{}{RESET}\n", rule.join(&" ".repeat(GAP))));
    } else {
        out.push_str(line.trim_end());
        out.push('\n');
    }

    for row in &cells {
        let mut line = String::new();
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                line.push_str(&" ".repeat(GAP));
            }
            let text = fit(cell, widths[index]);
            let width = if index == last && !right_align[index] {
                0
            } else {
                widths[index]
            };
            pad(&mut line, &text, width, right_align[index]);
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }

    if pretty && table.rows.is_empty() {
        out.push_str(&format!("{DIM}(no rows){RESET}\n"));
    }
    out
}

pub(crate) fn markdown(table: &Table) -> String {
    let escape = |text: String| text.replace('|', "\\|").replace('\n', " ");
    let mut out = String::new();
    out.push_str(&format!("| {} |\n", table.columns.join(" | ")));
    out.push_str(&format!("|{}\n", " --- |".repeat(table.columns.len())));
    for row in &table.rows {
        let cells: Vec<String> = row.iter().map(|value| escape(value.plain())).collect();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    out
}

pub(crate) fn html(table: &Table) -> String {
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let mut out = String::from("<table>\n  <thead>\n    <tr>");
    for column in &table.columns {
        out.push_str(&format!("<th>{}</th>", escape(column)));
    }
    out.push_str("</tr>\n  </thead>\n  <tbody>\n");
    for row in &table.rows {
        out.push_str("    <tr>");
        for value in row {
            out.push_str(&format!("<td>{}</td>", escape(&value.plain())));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("  </tbody>\n</table>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Table {
        let mut table = Table::new(vec!["name".into(), "size".into()]);
        table.push(vec![Value::Text("readme.md".into()), Value::Size(2048)]);
        table.push(vec![Value::Text("a".into()), Value::Size(10)]);
        table
    }

    #[test]
    fn plain_tables_align_and_right_justify_numbers() {
        let text = render(&sample(), Style::Plain, None, 0);
        let expected = [
            format!("name{}size", " ".repeat(9)),
            "readme.md  2.0 KB".to_string(),
            format!("a{}10 B", " ".repeat(12)),
        ];
        assert_eq!(text.lines().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn narrow_terminals_truncate_wide_columns() {
        let mut table = Table::new(vec!["path".into()]);
        table.push(vec![Value::Text(
            "/a/very/long/path/that/does/not/fit".into(),
        )]);
        let text = render(&table, Style::Plain, Some(12), 0);
        assert!(
            text.lines().all(|line| line.chars().count() <= 12),
            "{text}"
        );
        assert!(text.contains("..."));
    }

    #[test]
    fn html_output_is_escaped() {
        let mut table = Table::new(vec!["tag".into()]);
        table.push(vec![Value::Text("<b>&".into())]);
        assert!(html(&table).contains("<td>&lt;b&gt;&amp;</td>"));
    }

    #[test]
    fn markdown_output() {
        assert_eq!(
            markdown(&sample()),
            "| name | size |\n| --- | --- |\n| readme.md | 2.0 KB |\n| a | 10 B |\n"
        );
    }
}
