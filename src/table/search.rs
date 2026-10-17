use std::path::{Path, PathBuf};

use super::Table;
use super::ignore::{self, RuleSet};
use super::regex::Regex;
use super::value::Value;
use crate::expand::glob;

const BINARY_PROBE: usize = 8000;
const LARGEST_FILE: u64 = 20 * 1024 * 1024;
const LONGEST_TEXT: usize = 400;

enum Matcher {
    Fixed { needle: String, nocase: bool },
    Regex(Regex),
}

impl Matcher {
    fn new(pattern: &str, fixed: bool, nocase: bool) -> Result<Matcher, String> {
        if fixed {
            let needle = if nocase {
                pattern.to_lowercase()
            } else {
                pattern.to_string()
            };
            return Ok(Matcher::Fixed { needle, nocase });
        }
        Regex::new(pattern, nocase).map(Matcher::Regex)
    }

    fn matches(&self, line: &str) -> bool {
        match self {
            Matcher::Fixed {
                needle,
                nocase: true,
            } => line.to_lowercase().contains(needle.as_str()),
            Matcher::Fixed { needle, .. } => line.contains(needle.as_str()),
            Matcher::Regex(regex) => regex.is_match(line),
        }
    }
}

pub(super) struct Search {
    matcher: Matcher,
    hidden: bool,
    files_only: bool,
    include: Vec<String>,
    exclude: Vec<String>,
    roots: Vec<String>,
}

pub(super) struct Hit {
    pub path: String,
    pub line: usize,
    pub text: String,
}

pub(super) fn parse(args: &[String]) -> Result<Search, String> {
    let mut nocase = false;
    let mut fixed = false;
    let mut hidden = false;
    let mut files_only = false;
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    let mut positional = Vec::new();
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-i" | "--ignore-case" => nocase = true,
            "-F" | "--fixed" => fixed = true,
            "-H" | "--hidden" => hidden = true,
            "-l" | "--files" => files_only = true,
            "-g" | "--glob" => {
                let pattern = arguments.next().ok_or("-g needs a pattern")?;
                match pattern.strip_prefix('!') {
                    Some(negated) => exclude.push(negated.to_string()),
                    None => include.push(pattern.clone()),
                }
            }
            "--" => {
                positional.extend(arguments.by_ref().cloned());
            }
            flag if flag.starts_with('-') && flag.len() > 1 && positional.is_empty() => {
                return Err(format!("unknown flag '{flag}'"));
            }
            _ => positional.push(argument.clone()),
        }
    }
    let Some((pattern, roots)) = positional.split_first() else {
        return Err("usage: search [-i] [-F] [-H] [-l] [-g glob] pattern [path...]".into());
    };
    Ok(Search {
        matcher: Matcher::new(pattern, fixed, nocase)?,
        hidden,
        files_only,
        include,
        exclude,
        roots: if roots.is_empty() {
            vec![".".into()]
        } else {
            roots.to_vec()
        },
    })
}

impl Search {
    pub(super) fn run(&self) -> Result<(Vec<Hit>, bool), String> {
        let mut hits = Vec::new();
        for root in &self.roots {
            let path = Path::new(root);
            let metadata = std::fs::metadata(path)
                .map_err(|error| format!("{root}: {}", crate::sys::error_message(&error)))?;
            if metadata.is_file() {
                self.file(path, root.clone(), &mut hits);
                continue;
            }
            let absolute = std::fs::canonicalize(path)
                .map_err(|error| format!("{root}: {}", crate::sys::error_message(&error)))?;
            let mut sets = ignore::enclosing_sets(&absolute);
            let display = if root == "." {
                String::new()
            } else {
                root.trim_end_matches('/').to_string()
            };
            self.directory(&absolute, &display, &mut sets, &mut hits);
        }
        Ok((hits, self.files_only))
    }

    fn wanted(&self, name: &str) -> bool {
        (self.include.is_empty()
            || self
                .include
                .iter()
                .any(|pattern| glob::matches_with(pattern, name, false)))
            && !self
                .exclude
                .iter()
                .any(|pattern| glob::matches_with(pattern, name, false))
    }

    fn directory(
        &self,
        absolute: &Path,
        display: &str,
        sets: &mut Vec<RuleSet>,
        hits: &mut Vec<Hit>,
    ) {
        let pushed = RuleSet::load(absolute, &absolute.join(".gitignore"))
            .map(|set| sets.push(set))
            .is_some();
        let mut entries: Vec<(String, PathBuf, bool)> = match std::fs::read_dir(absolute) {
            Ok(entries) => entries
                .flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().into_string().ok()?;
                    let kind = entry.file_type().ok()?;
                    if kind.is_symlink() {
                        return None;
                    }
                    Some((name, entry.path(), kind.is_dir()))
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        entries.sort();
        for (name, path, is_directory) in entries {
            if name == ".git" || (!self.hidden && name.starts_with('.')) {
                continue;
            }
            if ignore::ignored(sets, &path, is_directory) {
                continue;
            }
            let shown = if display.is_empty() {
                name.clone()
            } else {
                format!("{display}/{name}")
            };
            if is_directory {
                self.directory(&path, &shown, sets, hits);
            } else if self.wanted(&name) {
                self.file(&path, shown, hits);
            }
        }
        if pushed {
            sets.pop();
        }
    }

    fn file(&self, path: &Path, shown: String, hits: &mut Vec<Hit>) {
        if std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > LARGEST_FILE) {
            return;
        }
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
            return;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (index, line) in text.lines().enumerate() {
            if !self.matcher.matches(line) {
                continue;
            }
            if self.files_only {
                hits.push(Hit {
                    path: shown,
                    line: index + 1,
                    text: String::new(),
                });
                return;
            }
            let line_text = match line.char_indices().nth(LONGEST_TEXT) {
                Some((cut, _)) => format!("{} [cut]", &line[..cut]),
                None => line.to_string(),
            };
            hits.push(Hit {
                path: shown.clone(),
                line: index + 1,
                text: line_text,
            });
        }
    }
}

pub(super) fn to_table(hits: &[Hit], files_only: bool) -> Table {
    if files_only {
        let mut table = Table::new(vec!["path".into()]);
        for hit in hits {
            table.push(vec![Value::Text(hit.path.clone())]);
        }
        return table;
    }
    let mut table = Table::new(vec!["path".into(), "line".into(), "text".into()]);
    for hit in hits {
        table.push(vec![
            Value::Text(hit.path.clone()),
            Value::Int(hit.line as i64),
            Value::Text(hit.text.trim().to_string()),
        ]);
    }
    table
}

pub(super) fn to_text(hits: &[Hit], files_only: bool) -> String {
    hits.iter()
        .map(|hit| {
            if files_only {
                format!("{}\n", hit.path)
            } else {
                format!("{}:{}:{}\n", hit.path, hit.line, hit.text)
            }
        })
        .collect()
}
