use std::path::PathBuf;

use rustyline::completion::Pair;

const SPECIAL: &str = " \t\n\"'\\$`&|;<>()*?[]{}!#";

#[derive(Debug, PartialEq)]
struct Entry {
    name: String,
    directory: bool,
}

fn unquote(raw: &str) -> (Option<char>, String) {
    let quote = raw
        .chars()
        .next()
        .filter(|first| *first == '\'' || *first == '"');
    let body = match quote {
        Some(open) => &raw[open.len_utf8()..],
        None => raw,
    };
    let mut text = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(character) = chars.next() {
        match (quote, character) {
            (Some('\''), _) => text.push(character),
            (Some(_), '\\') => match chars.next() {
                Some(escaped) if "\"\\$`".contains(escaped) => text.push(escaped),
                Some(other) => {
                    text.push('\\');
                    text.push(other);
                }
                None => text.push('\\'),
            },
            (None, '\\') => {
                if let Some(escaped) = chars.next() {
                    text.push(escaped);
                }
            }
            _ => text.push(character),
        }
    }
    (quote, text)
}

fn variable(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn expand_variables(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..dollar]);
        let after = &rest[dollar + 1..];
        if let Some(braced) = after.strip_prefix('{')
            && let Some(end) = braced.find('}')
        {
            out.push_str(&variable(&braced[..end]));
            rest = &braced[end + 1..];
            continue;
        }
        let length = after
            .find(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .unwrap_or(after.len());
        if length == 0 {
            out.push('$');
        } else {
            out.push_str(&variable(&after[..length]));
        }
        rest = &after[length..];
    }
    out.push_str(rest);
    out
}

fn expand_tilde(text: &str) -> String {
    let Some(rest) = text.strip_prefix('~') else {
        return text.to_string();
    };
    let (user, tail) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let home = if user.is_empty() {
        std::env::var("HOME").ok()
    } else {
        crate::sys::user_home(user)
    };
    match home {
        Some(home) => format!("{home}{tail}"),
        None => text.to_string(),
    }
}

fn lookup_directory(directory: &str, quote: Option<char>) -> PathBuf {
    if directory.is_empty() {
        return PathBuf::from(".");
    }
    let expanded = match quote {
        Some('\'') => directory.to_string(),
        Some(_) => expand_variables(directory),
        None => expand_variables(&expand_tilde(directory)),
    };
    PathBuf::from(expanded)
}

fn read_entries(directory: &PathBuf) -> Vec<Entry> {
    let Ok(listing) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    listing
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            let directory = std::fs::metadata(entry.path()).is_ok_and(|data| data.is_dir());
            Some(Entry { name, directory })
        })
        .collect()
}

fn select(entries: Vec<Entry>, prefix: &str, directories_only: bool) -> Vec<Entry> {
    let wanted: Vec<Entry> = entries
        .into_iter()
        .filter(|entry| !directories_only || entry.directory)
        .collect();
    let exact: Vec<&Entry> = wanted
        .iter()
        .filter(|entry| entry.name.starts_with(prefix))
        .collect();
    let lowered = prefix.to_lowercase();
    let matching: Vec<&Entry> = if exact.is_empty() {
        wanted
            .iter()
            .filter(|entry| entry.name.to_lowercase().starts_with(&lowered))
            .collect()
    } else {
        exact
    };
    let visible: Vec<&Entry> = matching
        .iter()
        .copied()
        .filter(|entry| !entry.name.starts_with('.'))
        .collect();
    let chosen = if prefix.starts_with('.') || visible.is_empty() {
        matching
    } else {
        visible
    };
    let mut chosen: Vec<Entry> = chosen
        .into_iter()
        .map(|entry| Entry {
            name: entry.name.clone(),
            directory: entry.directory,
        })
        .collect();
    chosen.sort_by(|left, right| left.name.cmp(&right.name));
    chosen
}

fn escape(name: &str, quote: Option<char>) -> String {
    match quote {
        Some('\'') => name.replace('\'', "'\\''"),
        Some(_) => name
            .chars()
            .flat_map(|character| {
                let escaped = "\"\\$`".contains(character);
                escaped.then_some('\\').into_iter().chain([character])
            })
            .collect(),
        None => name
            .chars()
            .flat_map(|character| {
                let escaped = SPECIAL.contains(character);
                escaped.then_some('\\').into_iter().chain([character])
            })
            .collect(),
    }
}

pub(super) fn complete(raw: &str, directories_only: bool) -> Vec<Pair> {
    let (quote, word) = unquote(raw);
    let (directory, prefix) = match word.rfind('/') {
        Some(slash) => word.split_at(slash + 1),
        None => ("", word.as_str()),
    };
    let raw_directory = match raw.rfind('/') {
        Some(slash) => &raw[..slash + 1],
        None => match quote {
            Some(open) => &raw[..open.len_utf8()],
            None => "",
        },
    };
    let entries = read_entries(&lookup_directory(directory, quote));
    select(entries, prefix, directories_only)
        .into_iter()
        .map(|entry| {
            let suffix = if entry.directory { "/" } else { "" };
            Pair {
                display: format!("{}{suffix}", entry.name),
                replacement: format!("{raw_directory}{}{suffix}", escape(&entry.name, quote)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, directory: bool) -> Entry {
        Entry {
            name: name.into(),
            directory,
        }
    }

    fn names(entries: Vec<Entry>) -> Vec<String> {
        entries.into_iter().map(|entry| entry.name).collect()
    }

    #[test]
    fn exact_case_wins_over_other_case() {
        let entries = vec![entry("Users", true), entry("usr", true), entry("var", true)];
        assert_eq!(names(select(entries, "us", false)), ["usr"]);
        let entries = vec![entry("Users", true), entry("var", true)];
        assert_eq!(names(select(entries, "us", false)), ["Users"]);
    }

    #[test]
    fn hidden_entries_only_for_a_dot_prefix() {
        let entries = || {
            vec![
                entry(".config", true),
                entry("src", true),
                entry(".git", true),
                entry("notes.txt", false),
            ]
        };
        assert_eq!(names(select(entries(), "", false)), ["notes.txt", "src"]);
        assert_eq!(names(select(entries(), "", true)), ["src"]);
        assert_eq!(names(select(entries(), ".", false)), [".config", ".git"]);
        let only_hidden = vec![entry(".cache", true)];
        assert_eq!(names(select(only_hidden, "", true)), [".cache"]);
    }

    #[test]
    fn variables_and_tilde_in_paths() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_variables("$HOME/pro"), format!("{home}/pro"));
        assert_eq!(expand_variables("${HOME}/x"), format!("{home}/x"));
        assert_eq!(expand_tilde("~/x"), format!("{home}/x"));
        assert_eq!(expand_variables("a$"), "a$");
    }

    #[test]
    fn replacements_keep_what_was_typed() {
        let directory = std::env::temp_dir().join(format!("grfsh_complete_{}", std::process::id()));
        std::fs::create_dir_all(directory.join("project dir")).unwrap();
        std::fs::write(directory.join("program.txt"), "").unwrap();
        let typed = format!("{}/pro", directory.display());
        let found: Vec<String> = complete(&typed, false)
            .into_iter()
            .map(|pair| pair.replacement)
            .collect();
        assert_eq!(
            found,
            [
                format!("{}/program.txt", directory.display()),
                format!("{}/project\\ dir/", directory.display())
            ]
        );
        let quoted: Vec<String> = complete(&format!("'{typed}"), true)
            .into_iter()
            .map(|pair| pair.replacement)
            .collect();
        assert_eq!(quoted, [format!("'{}/project dir/", directory.display())]);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn unquoting() {
        assert_eq!(unquote("my\\ fi"), (None, "my fi".to_string()));
        assert_eq!(unquote("'my fi"), (Some('\''), "my fi".to_string()));
        assert_eq!(unquote("\"a\\\"b"), (Some('"'), "a\"b".to_string()));
    }
}
