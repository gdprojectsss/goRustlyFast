use std::io::Write;

use crate::shell::{self, Shell};

#[derive(Debug, PartialEq)]
pub(super) struct Expansion {
    pub text: String,
    pub print_only: bool,
}

#[derive(Default)]
struct Substitution {
    old: String,
    new: String,
}

struct Expander<'a> {
    chars: Vec<char>,
    index: usize,
    history: &'a [String],
    out: String,
    print_only: bool,
    last_substitution: Option<Substitution>,
}

const OPERATOR_CHARS: &str = ";&|<>";

pub(super) fn words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(character) = chars.next() {
        if let Some(open) = quote {
            current.push(character);
            if character == open {
                quote = None;
            } else if character == '\\'
                && open == '"'
                && let Some(escaped) = chars.next()
            {
                current.push(escaped);
            }
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                current.push(character);
            }
            '\\' => {
                current.push(character);
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                }
            }
            character if character.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            '(' | ')' => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                words.push(character.to_string());
            }
            character if OPERATOR_CHARS.contains(character) => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                let mut operator = character.to_string();
                while let Some(&next) = chars.peek() {
                    if !OPERATOR_CHARS.contains(next) {
                        break;
                    }
                    operator.push(next);
                    chars.next();
                }
                words.push(operator);
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn not_found(designator: &str) -> String {
    format!("!{designator}: event not found")
}

fn event_by_number(history: &[String], number: i64) -> Option<&String> {
    if number < 0 {
        history
            .len()
            .checked_sub(number.unsigned_abs() as usize)
            .and_then(|index| history.get(index))
    } else {
        (number as usize)
            .checked_sub(1)
            .and_then(|index| history.get(index))
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn after_last_slash(text: &str) -> &str {
    text.rsplit_once('/').map_or(text, |(_, tail)| tail)
}

fn suffix_start(text: &str) -> Option<usize> {
    let base = text.len() - after_last_slash(text).len();
    text[base..].rfind('.').map(|dot| base + dot)
}

fn replace(text: &str, substitution: &Substitution, global: bool) -> Option<String> {
    if substitution.old.is_empty() || !text.contains(&substitution.old) {
        return None;
    }
    let new = substitution.new.replace('&', &substitution.old);
    Some(if global {
        text.replace(&substitution.old, &new)
    } else {
        text.replacen(&substitution.old, &new, 1)
    })
}

impl Expander<'_> {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.index + offset).copied()
    }

    fn take_while(&mut self, keep: impl Fn(char) -> bool) -> String {
        let start = self.index;
        while self.peek(0).is_some_and(&keep) {
            self.index += 1;
        }
        self.chars[start..self.index].iter().collect()
    }

    fn last_event(&self) -> Result<String, String> {
        self.history.last().cloned().ok_or_else(|| not_found("!"))
    }

    fn event(&mut self) -> Result<String, String> {
        match self.peek(0) {
            Some('!') => {
                self.index += 1;
                self.last_event()
            }
            Some('$' | '^' | '*' | ':' | '%') => self.last_event(),
            Some('#') => {
                self.index += 1;
                Ok(self.out.clone())
            }
            Some('?') => {
                self.index += 1;
                let needle = self.take_while(|character| character != '?' && character != '\n');
                if self.peek(0) == Some('?') {
                    self.index += 1;
                }
                self.history
                    .iter()
                    .rev()
                    .find(|line| line.contains(&needle))
                    .cloned()
                    .ok_or_else(|| not_found(&format!("?{needle}")))
            }
            Some(start) if start == '-' || start.is_ascii_digit() => {
                self.index += 1;
                let digits = self.take_while(|character| character.is_ascii_digit());
                let text = format!("{start}{digits}");
                let number = text.parse::<i64>().map_err(|_| not_found(&text))?;
                event_by_number(self.history, number)
                    .cloned()
                    .ok_or_else(|| not_found(&text))
            }
            _ => {
                let prefix = self.take_while(|character| {
                    !character.is_whitespace() && !";|&()<>'\":^$*%".contains(character)
                });
                self.history
                    .iter()
                    .rev()
                    .find(|line| line.starts_with(&prefix))
                    .cloned()
                    .ok_or_else(|| not_found(&prefix))
            }
        }
    }

    fn number(&mut self) -> Option<usize> {
        let digits = self.take_while(|character| character.is_ascii_digit());
        digits.parse().ok()
    }

    fn designator(&mut self, event: &str) -> Result<Option<String>, String> {
        let start = self.index;
        let explicit = self.peek(0) == Some(':')
            && self
                .peek(1)
                .is_some_and(|next| next.is_ascii_digit() || "^$*-%".contains(next));
        if explicit {
            self.index += 1;
        } else if !self.peek(0).is_some_and(|next| "^$*%".contains(next)) {
            return Ok(None);
        }
        let words = words(event);
        let last = words.len().saturating_sub(1);
        let range: Option<(usize, usize)> = match self.peek(0) {
            Some('^') => {
                self.index += 1;
                Some((1, 1))
            }
            Some('$') => {
                self.index += 1;
                Some((last, last))
            }
            Some('*') => {
                self.index += 1;
                if words.len() < 2 {
                    return Ok(Some(String::new()));
                }
                Some((1, last))
            }
            Some('-') => {
                self.index += 1;
                let end = if self.peek(0) == Some('$') {
                    self.index += 1;
                    Some(last)
                } else {
                    self.number()
                };
                end.map(|end| (0, end))
            }
            Some(digit) if digit.is_ascii_digit() => {
                let first = self.number().unwrap_or(0);
                match self.peek(0) {
                    Some('*') => {
                        self.index += 1;
                        Some((first, last))
                    }
                    Some('-') => {
                        self.index += 1;
                        if self.peek(0) == Some('$') {
                            self.index += 1;
                            Some((first, last))
                        } else if self.peek(0).is_some_and(|next| next.is_ascii_digit()) {
                            self.number().map(|end| (first, end))
                        } else {
                            Some((first, last.saturating_sub(1)))
                        }
                    }
                    _ => Some((first, first)),
                }
            }
            _ => None,
        };
        let specifier: String = self.chars[start..self.index].iter().collect();
        let bad = || format!("{specifier}: bad word specifier");
        let (first, end) = range.ok_or_else(bad)?;
        if words.is_empty() || first > end || end > last {
            return Err(bad());
        }
        Ok(Some(words[first..=end].join(" ")))
    }

    fn delimited(&mut self, delimiter: char) -> String {
        let mut text = String::new();
        while let Some(character) = self.peek(0) {
            self.index += 1;
            if character == delimiter {
                break;
            }
            if character == '\\' && self.peek(0) == Some(delimiter) {
                text.push(delimiter);
                self.index += 1;
                continue;
            }
            text.push(character);
        }
        text
    }

    fn substitute(&mut self, text: &str, global: bool) -> Result<String, String> {
        let Some(delimiter) = self.peek(0) else {
            return Err(":s: substitution failed".into());
        };
        self.index += 1;
        let old = self.delimited(delimiter);
        let new = self.delimited(delimiter);
        let previous = self.last_substitution.take().unwrap_or_default();
        let substitution = Substitution {
            old: if old.is_empty() { previous.old } else { old },
            new,
        };
        let result = replace(text, &substitution, global);
        let failure = format!(
            ":s{delimiter}{}{delimiter}{}: substitution failed",
            substitution.old, substitution.new
        );
        self.last_substitution = Some(substitution);
        result.ok_or(failure)
    }

    fn repeat_substitution(&mut self, text: &str, global: bool) -> Result<String, String> {
        let substitution = self.last_substitution.take().unwrap_or_default();
        let result = replace(text, &substitution, global);
        self.last_substitution = Some(substitution);
        result.ok_or_else(|| ":&: substitution failed".into())
    }

    fn modifiers(&mut self, mut text: String) -> Result<String, String> {
        while self.peek(0) == Some(':') {
            let Some(modifier) = self.peek(1) else {
                break;
            };
            if !"htreqxpsg&aG".contains(modifier) {
                break;
            }
            self.index += 2;
            text = match modifier {
                'h' => match text.rsplit_once('/') {
                    Some((head, _)) => head.to_string(),
                    None => text,
                },
                't' => after_last_slash(&text).to_string(),
                'r' => match suffix_start(&text) {
                    Some(dot) => text[..dot].to_string(),
                    None => text,
                },
                'e' => match suffix_start(&text) {
                    Some(dot) => text[dot..].to_string(),
                    None => text,
                },
                'q' => quote(&text),
                'x' => text
                    .split_whitespace()
                    .map(quote)
                    .collect::<Vec<_>>()
                    .join(" "),
                'p' => {
                    self.print_only = true;
                    text
                }
                's' => self.substitute(&text, false)?,
                '&' => self.repeat_substitution(&text, false)?,
                _ => {
                    let next = self.peek(0);
                    self.index += 1;
                    match next {
                        Some('s') => self.substitute(&text, true)?,
                        Some('&') => self.repeat_substitution(&text, true)?,
                        _ => return Err(format!(":{modifier}: unrecognized history modifier")),
                    }
                }
            };
        }
        Ok(text)
    }

    fn reference(&mut self) -> Result<String, String> {
        let event = self.event()?;
        let selected = self.designator(&event)?.unwrap_or(event);
        self.modifiers(selected)
    }

    fn starts_reference(&self) -> bool {
        let previous = self.index.checked_sub(1).map(|index| self.chars[index]);
        let before_previous = self.index.checked_sub(2).map(|index| self.chars[index]);
        let inhibited = match previous {
            Some('$' | '[') => true,
            Some('{') => before_previous == Some('$'),
            _ => false,
        };
        match self.peek(1) {
            None => false,
            Some(_) if inhibited => false,
            Some(next) => !next.is_whitespace() && !"=()\";&|<>`[]{}/".contains(next),
        }
    }

    fn quick_substitution(&mut self) -> Result<(), String> {
        self.index = 1;
        let old = self.delimited('^');
        let new = self.delimited('^');
        let event = self.last_event()?;
        let substitution = Substitution { old, new };
        let failure = format!(
            ":s^{}^{}: substitution failed",
            substitution.old, substitution.new
        );
        let replaced = replace(&event, &substitution, false).ok_or(failure)?;
        self.last_substitution = Some(substitution);
        let replaced = self.modifiers(replaced)?;
        self.out.push_str(&replaced);
        Ok(())
    }

    fn run(&mut self) -> Result<bool, String> {
        let mut changed = false;
        if self.chars.first() == Some(&'^') && self.chars.iter().skip(1).any(|&c| c == '^') {
            self.quick_substitution()?;
            changed = true;
        }
        let mut in_single = false;
        let mut in_double = false;
        while let Some(character) = self.peek(0) {
            if character == '\'' && !in_double {
                in_single = !in_single;
            } else if character == '"' && !in_single {
                in_double = !in_double;
            }
            if character == '\\' && self.peek(1).is_some() {
                self.out.push(character);
                self.out.push(self.chars[self.index + 1]);
                self.index += 2;
                continue;
            }
            if character != '!' || in_single || !self.starts_reference() {
                self.out.push(character);
                self.index += 1;
                continue;
            }
            self.index += 1;
            let replacement = self.reference()?;
            self.out.push_str(&replacement);
            changed = true;
        }
        Ok(changed)
    }
}

pub(super) fn expand(line: &str, history: &[String]) -> Result<Option<Expansion>, String> {
    if !line.contains('!') && !line.starts_with('^') {
        return Ok(None);
    }
    let mut expander = Expander {
        chars: line.chars().collect(),
        index: 0,
        history,
        out: String::with_capacity(line.len()),
        print_only: false,
        last_substitution: None,
    };
    let changed = expander.run()?;
    Ok(changed.then_some(Expansion {
        text: expander.out,
        print_only: expander.print_only,
    }))
}

pub(super) fn last_argument(line: &str) -> Option<String> {
    words(line).pop()
}

struct FcOptions {
    list: bool,
    numbers: bool,
    reverse: bool,
    rerun: bool,
    editor: Option<String>,
    operands: Vec<String>,
}

fn fc_options(args: &[String]) -> Result<FcOptions, String> {
    let mut options = FcOptions {
        list: false,
        numbers: true,
        reverse: false,
        rerun: false,
        editor: None,
        operands: Vec::new(),
    };
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        index += 1;
        if argument == "--" {
            break;
        }
        let is_option = argument.len() > 1
            && argument.starts_with('-')
            && !argument[1..].starts_with(|character: char| character.is_ascii_digit());
        if !is_option {
            index -= 1;
            break;
        }
        for flag in argument[1..].chars() {
            match flag {
                'l' => options.list = true,
                'n' => options.numbers = false,
                'r' => options.reverse = true,
                's' => options.rerun = true,
                'e' => {
                    let editor = args
                        .get(index)
                        .ok_or("fc: -e: option requires an argument")?;
                    index += 1;
                    if editor == "-" {
                        options.rerun = true;
                    } else {
                        options.editor = Some(editor.clone());
                    }
                }
                _ => {
                    return Err(format!(
                        "fc: -{flag}: invalid option\nfc: usage: fc [-e ename] [-lnr] [first] [last] or fc -s [pat=rep] [command]"
                    ));
                }
            }
        }
    }
    options.operands = args[index..].to_vec();
    Ok(options)
}

fn fc_entry(entries: &[String], spec: &str, current: usize) -> Option<i64> {
    if let Ok(number) = spec.parse::<i64>() {
        return Some(match number {
            number if number < 0 => current as i64 + number,
            0 => current as i64 - 1,
            number => number,
        });
    }
    entries
        .iter()
        .rposition(|entry| entry.starts_with(spec))
        .map(|index| index as i64 + 1)
}

fn fc_range(
    entries: &[String],
    first: Option<&str>,
    last: Option<&str>,
    default_first: &str,
) -> Result<(i64, i64), String> {
    let current = entries.len() + 1;
    let out_of_range = || "fc: history specification out of range".to_string();
    let first_spec = first.unwrap_or(default_first);
    let first = fc_entry(entries, first_spec, current).ok_or_else(out_of_range)?;
    let last = match last {
        Some(spec) => fc_entry(entries, spec, current).ok_or_else(out_of_range)?,
        None if default_first == "-16" => current as i64 - 1,
        None => first,
    };
    Ok((first, last))
}

fn checked_position(entries: &[String], position: i64) -> Option<usize> {
    (1..=entries.len() as i64)
        .contains(&position)
        .then_some(position as usize)
}

fn fc_list(entries: &[String], options: &FcOptions) -> i32 {
    if entries.is_empty() {
        return 0;
    }
    let first = options.operands.first().map(String::as_str);
    let last = options.operands.get(1).map(String::as_str);
    let (first, last) = match fc_range(entries, first, last, "-16") {
        Ok(range) => range,
        Err(message) => {
            shell::error(message);
            return 1;
        }
    };
    let clamp = |position: i64| position.clamp(1, entries.len() as i64) as usize;
    let (first, last) = (clamp(first), clamp(last));
    let mut positions: Vec<usize> = if first <= last {
        (first..=last).collect()
    } else {
        (last..=first).rev().collect()
    };
    if options.reverse {
        positions.reverse();
    }
    let mut out = std::io::stdout().lock();
    for position in positions {
        let entry = &entries[position - 1];
        let _ = if options.numbers {
            writeln!(out, "{position}\t {entry}")
        } else {
            writeln!(out, "\t {entry}")
        };
    }
    let _ = out.flush();
    0
}

fn fc_rerun(shell: &mut Shell, entries: &[String], options: &FcOptions) -> i32 {
    let mut operands = options.operands.as_slice();
    let mut replacements = Vec::new();
    while let Some((pattern, replacement)) = operands.first().and_then(|word| word.split_once('='))
    {
        replacements.push((pattern.to_string(), replacement.to_string()));
        operands = &operands[1..];
    }
    let spec = operands.first().map_or("-1", String::as_str);
    let current = entries.len() + 1;
    let Some(command) = fc_entry(entries, spec, current)
        .and_then(|position| checked_position(entries, position))
        .map(|position| &entries[position - 1])
    else {
        shell::error("fc: no command found");
        return 1;
    };
    let mut command = command.clone();
    for (pattern, replacement) in &replacements {
        if !pattern.is_empty() {
            command = command.replace(pattern.as_str(), replacement);
        }
    }
    run_from_history(shell, &command)
}

fn run_from_history(shell: &mut Shell, commands: &str) -> i32 {
    let commands = commands.trim_end_matches('\n');
    eprintln!("{commands}");
    if shell.interactive {
        shell.history.push(commands.to_string());
    }
    shell.run_source(commands)
}

fn fc_edit(shell: &mut Shell, entries: &[String], options: &FcOptions) -> i32 {
    let first = options.operands.first().map(String::as_str);
    let last = options.operands.get(1).map(String::as_str);
    let range = fc_range(entries, first, last, "-1")
        .ok()
        .and_then(|(first, last)| {
            Some((
                checked_position(entries, first)?,
                checked_position(entries, last)?,
            ))
        });
    let (first, last) = match range {
        Some(range) => range,
        None => {
            shell::error("fc: history specification out of range");
            return 1;
        }
    };
    let mut selected: Vec<&String> = if first <= last {
        entries[first - 1..last].iter().collect()
    } else {
        entries[last - 1..first].iter().rev().collect()
    };
    if options.reverse {
        selected.reverse();
    }
    let directory = crate::bytes::env_var("TMPDIR").unwrap_or_else(|| "/tmp".into());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let path = std::path::Path::new(&directory)
        .join(format!("grfsh-fc-{}-{stamp}.sh", crate::sys::getpid()));
    let contents: String = selected.iter().map(|entry| format!("{entry}\n")).collect();
    if let Err(error) = crate::sys::write_private(&path, contents.as_bytes()) {
        shell::error(format!(
            "fc: {}: {}",
            path.display(),
            crate::sys::error_message(&error)
        ));
        return 1;
    }
    let editor = options
        .editor
        .clone()
        .or_else(|| {
            shell
                .get_variable("FCEDIT")
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            shell
                .get_variable("EDITOR")
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "vi".into());
    let path_text = path.display().to_string();
    let status = shell.run_source(&format!("{editor} {}", quote(&path_text)));
    let edited = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    if status != 0 {
        return status;
    }
    if edited.trim().is_empty() {
        return 0;
    }
    run_from_history(shell, &edited)
}

pub(crate) fn fc(shell: &mut Shell, args: &[String]) -> i32 {
    let options = match fc_options(args) {
        Ok(options) => options,
        Err(message) => {
            shell::error(message);
            return 2;
        }
    };
    let count = if shell.interactive {
        shell.history.len().saturating_sub(1)
    } else {
        shell.history.len()
    };
    let entries: Vec<String> = shell.history[..count].to_vec();
    if options.list {
        fc_list(&entries, &options)
    } else if options.rerun {
        fc_rerun(shell, &entries, &options)
    } else if entries.is_empty() {
        shell::error("fc: history specification out of range");
        1
    } else {
        fc_edit(shell, &entries, &options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> Vec<String> {
        vec![
            "git status".into(),
            "cargo build --release".into(),
            "ls -la src".into(),
        ]
    }

    fn expanded(line: &str, history: &[String]) -> Option<String> {
        expand(line, history)
            .unwrap()
            .map(|expansion| expansion.text)
    }

    #[test]
    fn previous_command_and_words() {
        let history = history();
        assert_eq!(
            expanded("sudo !!", &history).as_deref(),
            Some("sudo ls -la src")
        );
        assert_eq!(expanded("cd !$", &history).as_deref(), Some("cd src"));
        assert_eq!(expanded("echo !^", &history).as_deref(), Some("echo -la"));
        assert_eq!(
            expanded("echo !*", &history).as_deref(),
            Some("echo -la src")
        );
    }

    #[test]
    fn events_by_number_and_prefix() {
        let history = history();
        assert_eq!(expanded("!1", &history).as_deref(), Some("git status"));
        assert_eq!(
            expanded("!-2", &history).as_deref(),
            Some("cargo build --release")
        );
        assert_eq!(
            expanded("!car", &history).as_deref(),
            Some("cargo build --release")
        );
        assert_eq!(
            expanded("!?release?", &history).as_deref(),
            Some("cargo build --release")
        );
        assert_eq!(
            expand("!nothing", &history).unwrap_err(),
            "!nothing: event not found"
        );
    }

    #[test]
    fn literal_bangs_stay() {
        let history = history();
        assert_eq!(expanded("echo hi", &history), None);
        assert_eq!(expanded("echo '!!' done!", &history), None);
        assert_eq!(expanded("[ ! -d x ]", &history), None);
        assert_eq!(expanded("echo \\!!", &history), None);
        assert_eq!(expanded("[ x != y ]", &history), None);
        assert_eq!(expanded("echo x!=1", &history), None);
        assert_eq!(expanded("echo \"hi!\"", &history), None);
        assert_eq!(expanded("echo ${!name} $! $!; ls [!a]*", &history), None);
        assert_eq!(expanded("echo hi!;", &history), None);
    }

    #[test]
    fn word_designators() {
        let history = vec!["echo x y z".to_string()];
        assert_eq!(
            expanded("echo !!:1-2 !!:2* !!:0", &history).as_deref(),
            Some("echo x y y z echo")
        );
        assert_eq!(expanded("!!:1", &history).as_deref(), Some("x"));
        assert_eq!(expanded("!:$", &history).as_deref(), Some("z"));
        assert_eq!(expanded("!!:-1", &history).as_deref(), Some("echo x"));
        assert_eq!(expanded("!!:1-", &history).as_deref(), Some("x y"));
        assert_eq!(expanded("!!:*", &history).as_deref(), Some("x y z"));
        assert_eq!(
            expand("echo !!:9", &history).unwrap_err(),
            ":9: bad word specifier"
        );
        let quoted = vec!["echo \"a b\" c".to_string()];
        assert_eq!(
            expanded("echo !!:1", &quoted).as_deref(),
            Some("echo \"a b\"")
        );
    }

    #[test]
    fn modifiers() {
        let history = vec!["echo /x/a/b.tar.gz".to_string()];
        assert_eq!(
            expanded("echo !$:e !$:r !$:h !$:t !$:h:h", &history).as_deref(),
            Some("echo .gz /x/a/b.tar /x/a b.tar.gz /x")
        );
        let plain = vec!["echo d".to_string()];
        assert_eq!(
            expanded("echo !$:e !$:h", &plain).as_deref(),
            Some("echo d d")
        );
        let quoted = vec!["echo \"q x\" y".to_string()];
        assert_eq!(
            expanded("echo !!:*:q !!:1:x", &quoted).as_deref(),
            Some("echo '\"q x\" y' '\"q' 'x\"'")
        );
        let repeated = vec!["echo a a a".to_string()];
        assert_eq!(
            expanded("!!:gs/a/b/", &repeated).as_deref(),
            Some("echo b b b")
        );
        assert_eq!(
            expanded("!!:s/a/b/:&", &repeated).as_deref(),
            Some("echo b b a")
        );
        let printed = expand("!!:p", &repeated).unwrap().unwrap();
        assert!(printed.print_only);
        assert_eq!(printed.text, "echo a a a");
    }

    #[test]
    fn quick_substitution() {
        let history = vec!["echo x y x".to_string()];
        assert_eq!(expanded("^x^Q", &history).as_deref(), Some("echo Q y x"));
        assert_eq!(
            expanded("^x^Q^ z", &history).as_deref(),
            Some("echo Q y x z")
        );
        assert_eq!(
            expand("^zz^y", &history).unwrap_err(),
            ":s^zz^y: substitution failed"
        );
    }

    #[test]
    fn history_words_keep_quotes_and_split_operators() {
        assert_eq!(
            words("echo 'a b' \"c d\"|wc -l&&ls"),
            ["echo", "'a b'", "\"c d\"", "|", "wc", "-l", "&&", "ls"]
        );
        assert_eq!(
            last_argument("cp a.txt /tmp/dir").as_deref(),
            Some("/tmp/dir")
        );
    }

    #[test]
    fn fc_ranges() {
        let entries: Vec<String> = (1..=20).map(|number| format!("echo {number}")).collect();
        assert_eq!(fc_range(&entries, None, None, "-16"), Ok((5, 20)));
        assert_eq!(fc_range(&entries, Some("-3"), None, "-16"), Ok((18, 20)));
        assert_eq!(fc_range(&entries, Some("2"), Some("4"), "-16"), Ok((2, 4)));
        assert_eq!(fc_range(&entries, Some("echo 7"), None, "-1"), Ok((7, 7)));
        assert!(fc_range(&entries, Some("nope"), None, "-1").is_err());
    }

    #[test]
    fn fc_option_parsing() {
        let args: Vec<String> = ["-lnr", "-5"].iter().map(|word| word.to_string()).collect();
        let options = fc_options(&args).unwrap();
        assert!(options.list && !options.numbers && options.reverse);
        assert_eq!(options.operands, ["-5"]);
        let args: Vec<String> = ["-e", "-", "a=b"]
            .iter()
            .map(|word| word.to_string())
            .collect();
        let options = fc_options(&args).unwrap();
        assert!(options.rerun);
        assert_eq!(options.operands, ["a=b"]);
    }
}
