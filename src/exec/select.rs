use std::io::Write;

use crate::expand;
use crate::shell::{self, Shell};
use crate::syntax::ast::{List, Word};
use crate::sys;

const SEPARATOR: &str = ") ";
const TAB_SIZE: usize = 8;

fn number_length(number: usize) -> usize {
    number.to_string().len()
}

fn indent(out: &mut String, mut from: usize, to: usize) {
    while from < to {
        if to / TAB_SIZE > from / TAB_SIZE {
            out.push('\t');
            from += TAB_SIZE - from % TAB_SIZE;
        } else {
            out.push(' ');
            from += 1;
        }
    }
}

pub(crate) fn menu(items: &[String], columns: usize) -> String {
    let count = items.len();
    let indices_length = number_length(count);
    let widest = items
        .iter()
        .map(|item| item.chars().count())
        .max()
        .unwrap_or(0);
    let cell = widest + indices_length + SEPARATOR.len() + 2;
    let mut column_count = (columns / cell).max(1);
    let mut rows = count.div_ceil(column_count).max(1);
    column_count = count.div_ceil(rows).max(1);
    if rows == 1 {
        rows = column_count;
    }
    let first_indices_length = number_length(rows);
    let mut out = String::new();
    for row in 0..rows {
        let mut index = row;
        let mut position = 0;
        loop {
            let width = if position == 0 {
                first_indices_length
            } else {
                indices_length
            };
            let item = &items[index];
            out.push_str(&format!("{:>width$}{SEPARATOR}{item}", index + 1));
            let length = width + SEPARATOR.len() + item.chars().count();
            index += rows;
            if index >= count {
                break;
            }
            indent(&mut out, position + length, position + cell);
            position += cell;
        }
        out.push('\n');
    }
    out
}

fn read_reply() -> Option<String> {
    let mut reply = String::new();
    loop {
        let line = sys::read_line(0)?;
        let line = line.strip_suffix('\n').unwrap_or(&line);
        let mut chars = line.chars();
        let mut continued = false;
        while let Some(character) = chars.next() {
            if character != '\\' {
                reply.push(character);
                continue;
            }
            match chars.next() {
                Some(escaped) => reply.push(escaped),
                None => continued = true,
            }
        }
        if !continued {
            return Some(reply);
        }
    }
}

fn selection(reply: &str, items: &[String]) -> String {
    reply
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|choice| (1..=items.len()).contains(choice))
        .map(|choice| items[choice - 1].clone())
        .unwrap_or_default()
}

impl Shell {
    fn select_columns(&self) -> usize {
        self.get_variable("COLUMNS")
            .and_then(|columns| columns.trim().parse().ok())
            .filter(|columns| *columns > 0)
            .unwrap_or(80)
    }

    fn select_query(&mut self, items: &[String], mut show_menu: bool) -> Option<String> {
        loop {
            let prompt = self.get_variable("PS3").unwrap_or_else(|| "#? ".into());
            let mut text = String::new();
            if show_menu {
                text = menu(items, self.select_columns());
            }
            text.push_str(&prompt);
            let mut error = std::io::stderr();
            let _ = error.write_all(text.as_bytes());
            let _ = error.flush();
            let Some(reply) = read_reply() else {
                println!();
                return None;
            };
            self.set_variable("REPLY", &reply);
            if reply.is_empty() {
                show_menu = true;
                continue;
            }
            return Some(selection(&reply, items));
        }
    }

    pub(crate) fn run_select(
        &mut self,
        variable: &str,
        items: Option<&[Word]>,
        body: &List,
    ) -> i32 {
        let items = match items {
            Some(words) => match expand::words(self, words) {
                Ok(values) => values,
                Err(error) => {
                    shell::error(error);
                    return 1;
                }
            },
            None => self.positional.clone(),
        };
        if items.is_empty() {
            return 0;
        }
        let mut show_menu = true;
        self.loop_depth += 1;
        let status = loop {
            let Some(choice) = self.select_query(&items, show_menu) else {
                break 1;
            };
            show_menu = false;
            if let Err(error) = self.try_set_variable(variable, &choice) {
                shell::error(error);
                break 1;
            }
            let status = self.run_list(body);
            if self.loop_should_exit() {
                break status;
            }
        };
        self.loop_depth -= 1;
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(count: usize) -> Vec<String> {
        (0..count)
            .map(|index| char::from(b'a' + index as u8).to_string())
            .collect()
    }

    #[test]
    fn short_lists_are_vertical() {
        assert_eq!(menu(&names(3), 80), "1) a\n2) b\n3) c\n");
    }

    #[test]
    fn long_lists_use_columns_like_bash() {
        let expected = "1) a    4) d   7) g  10) j  13) m  16) p  19) s\t 22) v\t25) y\n\
                        2) b    5) e   8) h  11) k  14) n  17) q  20) t\t 23) w\t26) z\n\
                        3) c    6) f   9) i  12) l  15) o  18) r  21) u\t 24) x\n";
        assert_eq!(menu(&names(26), 80), expected);
    }

    #[test]
    fn replies_select_by_number() {
        let items = names(2);
        assert_eq!(selection(" 2 ", &items), "b");
        assert_eq!(selection("3", &items), "");
        assert_eq!(selection("x", &items), "");
    }
}
