use std::path::{Path, PathBuf};

use crate::expand::glob;

#[derive(Debug, Clone, PartialEq)]
struct Rule {
    segments: Vec<String>,
    negate: bool,
    directory_only: bool,
}

#[derive(Debug, Clone)]
pub(super) struct RuleSet {
    base: PathBuf,
    rules: Vec<Rule>,
}

fn parse_rule(line: &str) -> Option<Rule> {
    let line = line.trim_end();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (negate, line) = match line.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, line.strip_prefix('\\').unwrap_or(line)),
    };
    let directory_only = line.ends_with('/');
    let line = line.trim_end_matches('/');
    if line.is_empty() {
        return None;
    }
    let anchored = line.contains('/');
    let mut segments: Vec<String> = line
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(String::from)
        .collect();
    if !anchored {
        segments.insert(0, "**".into());
    }
    Some(Rule {
        segments,
        negate,
        directory_only,
    })
}

fn segments_match(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first == "**" => {
            (0..=path.len()).any(|skip| segments_match(rest, &path[skip..]))
        }
        Some((first, rest)) => {
            !path.is_empty()
                && glob::matches_with(first, path[0], false)
                && segments_match(rest, &path[1..])
        }
    }
}

impl RuleSet {
    pub(super) fn parse(base: &Path, text: &str) -> RuleSet {
        RuleSet {
            base: base.to_path_buf(),
            rules: text.lines().filter_map(parse_rule).collect(),
        }
    }

    pub(super) fn load(base: &Path, file: &Path) -> Option<RuleSet> {
        let text = std::fs::read_to_string(file).ok()?;
        Some(RuleSet::parse(base, &text))
    }

    fn verdict(&self, path: &Path, directory: bool) -> Option<bool> {
        let relative = path.strip_prefix(&self.base).ok()?;
        let parts: Vec<&str> = relative.iter().filter_map(|part| part.to_str()).collect();
        if parts.is_empty() {
            return None;
        }
        self.rules
            .iter()
            .rev()
            .find(|rule| {
                (directory || !rule.directory_only) && segments_match(&rule.segments, &parts)
            })
            .map(|rule| !rule.negate)
    }
}

pub(super) fn ignored(sets: &[RuleSet], path: &Path, directory: bool) -> bool {
    sets.iter()
        .rev()
        .find_map(|set| set.verdict(path, directory))
        .unwrap_or(false)
}

pub(super) fn enclosing_sets(root: &Path) -> Vec<RuleSet> {
    let mut sets = Vec::new();
    let mut directory = root.parent();
    let mut chain = Vec::new();
    while let Some(current) = directory {
        chain.push(current.to_path_buf());
        if current.join(".git").exists() {
            break;
        }
        directory = current.parent();
    }
    if !chain.last().is_some_and(|top| top.join(".git").exists()) && !root.join(".git").exists() {
        return sets;
    }
    for directory in chain.iter().rev() {
        if directory.join(".git").exists() {
            sets.extend(RuleSet::load(
                directory,
                &directory.join(".git/info/exclude"),
            ));
        }
        sets.extend(RuleSet::load(directory, &directory.join(".gitignore")));
    }
    sets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(rules: &str, path: &str, directory: bool) -> bool {
        let set = RuleSet::parse(Path::new("/repo"), rules);
        ignored(&[set], &Path::new("/repo").join(path), directory)
    }

    #[test]
    fn names_match_at_any_depth() {
        assert!(check("target\n", "target", true));
        assert!(check("*.log\n", "a/b/c.log", false));
        assert!(!check("*.log\n", "a/b/c.txt", false));
    }

    #[test]
    fn anchored_and_directory_rules() {
        assert!(check("/build\n", "build", true));
        assert!(!check("/build\n", "src/build", true));
        assert!(check("docs/*.html\n", "docs/a.html", false));
        assert!(!check("cache/\n", "cache", false));
        assert!(check("cache/\n", "cache", true));
        assert!(check("a/**/z\n", "a/b/c/z", false));
    }

    #[test]
    fn negation_wins_when_later() {
        assert!(!check("*.log\n!keep.log\n", "keep.log", false));
        assert!(check("!keep.log\n*.log\n", "keep.log", false));
        assert!(!check("# comment\n\n", "x", false));
    }
}
