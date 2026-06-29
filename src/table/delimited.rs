pub(crate) fn parse(text: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut field_started = false;
    let mut chars = text.chars().peekable();
    let mut line = 1;

    while let Some(character) = chars.next() {
        if in_quotes {
            match character {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                other => field.push(other),
            }
            continue;
        }
        match character {
            '"' if !field_started => {
                in_quotes = true;
                field_started = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                line += 1;
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                field_started = false;
            }
            separator if separator == delimiter => {
                record.push(std::mem::take(&mut field));
                field_started = false;
            }
            other => {
                field.push(other);
                field_started = true;
            }
        }
    }
    if in_quotes {
        return Err(format!("unterminated quoted field near line {line}"));
    }
    if field_started || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records.retain(|record| !(record.len() == 1 && record[0].is_empty()));
    Ok(records)
}

pub(crate) fn write_field(out: &mut String, field: &str, delimiter: char) {
    let needs_quotes = field.contains([delimiter, '"', '\n', '\r'])
        || field.starts_with(' ')
        || field.ends_with(' ');
    if needs_quotes {
        out.push('"');
        out.push_str(&field.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(field);
    }
}

pub(crate) fn write_record<'a>(
    out: &mut String,
    fields: impl IntoIterator<Item = &'a str>,
    delimiter: char,
) {
    for (index, field) in fields.into_iter().enumerate() {
        if index > 0 {
            out.push(delimiter);
        }
        write_field(out, field, delimiter);
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_fields_and_escaped_quotes() {
        let records = parse("name,note\n\"Smith, J\",\"said \"\"hi\"\"\"\n", ',').unwrap();
        assert_eq!(records[1], ["Smith, J", "said \"hi\""]);
    }

    #[test]
    fn crlf_and_missing_final_newline() {
        let records = parse("a,b\r\n1,2", ',').unwrap();
        assert_eq!(records, vec![vec!["a", "b"], vec!["1", "2"]]);
    }

    #[test]
    fn empty_fields_survive() {
        let records = parse("a,,c\n", ',').unwrap();
        assert_eq!(records[0], ["a", "", "c"]);
    }

    #[test]
    fn tabs() {
        let records = parse("x\ty\n1\t2\n", '\t').unwrap();
        assert_eq!(records[1], ["1", "2"]);
    }

    #[test]
    fn unterminated_quote_is_an_error() {
        assert!(parse("\"open,field\n", ',').is_err());
    }

    #[test]
    fn writing_quotes_when_needed() {
        let mut out = String::new();
        write_record(&mut out, ["plain", "has,comma", "has \"quote\""], ',');
        assert_eq!(out, "plain,\"has,comma\",\"has \"\"quote\"\"\"\n");
    }
}
