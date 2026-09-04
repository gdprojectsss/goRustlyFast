use crate::syntax::ast::Anchor;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GlobOptions {
    pub dotglob: bool,
    pub nocase: bool,
    pub globstar: bool,
}

fn visible(name: &str, component: &str, options: GlobOptions) -> bool {
    !name.starts_with('.')
        || component.starts_with('.')
        || (options.dotglob && name != "." && name != "..")
}

fn descendants(base: &str, include_files: bool, options: GlobOptions, out: &mut Vec<String>) {
    let directory = if base.is_empty() { "." } else { base };
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !visible(&name, "", options) {
            continue;
        }
        let path = join(base, &name);
        let is_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if is_directory || include_files {
            out.push(path.clone());
        }
        if is_directory {
            descendants(&path, include_files, options, out);
        }
    }
}

pub fn expand(pattern: &str, options: GlobOptions) -> Option<Vec<String>> {
    let mut paths = vec![if pattern.starts_with('/') {
        "/".to_string()
    } else {
        String::new()
    }];
    let components: Vec<&str> = pattern.split('/').filter(|part| !part.is_empty()).collect();

    for (position, component) in components.iter().enumerate() {
        let last = position + 1 == components.len();
        let mut next = Vec::new();
        if options.globstar && *component == "**" {
            for base in &paths {
                if !last {
                    next.push(base.clone());
                }
                descendants(base, last, options, &mut next);
            }
        } else if !has_magic(component) {
            let name = unescape(component);
            next.extend(paths.iter().map(|base| join(base, &name)));
        } else {
            for base in &paths {
                let directory = if base.is_empty() { "." } else { base.as_str() };
                let Ok(entries) = std::fs::read_dir(directory) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let Ok(name) = entry.file_name().into_string() else {
                        continue;
                    };
                    if !visible(&name, component, options) {
                        continue;
                    }
                    if matches_with(component, &name, options.nocase) {
                        next.push(join(base, &name));
                    }
                }
            }
        }
        paths = next;
    }

    if pattern.ends_with('/') {
        paths.retain(|path| std::path::Path::new(path).is_dir());
        paths.iter_mut().for_each(|path| path.push('/'));
    }
    paths.retain(|path| {
        !path.is_empty()
            && (std::fs::symlink_metadata(path.trim_end_matches('/')).is_ok() || path == "/")
    });
    if paths.is_empty() {
        return None;
    }
    paths.sort();
    paths.dedup();
    Some(paths)
}

pub fn matches_with(pattern: &str, text: &str, nocase: bool) -> bool {
    if nocase {
        matches_pattern(&pattern.to_lowercase(), &text.to_lowercase())
    } else {
        matches_pattern(pattern, text)
    }
}

fn join(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

pub fn has_magic(pattern: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '\\' => index += 1,
            '*' | '?' => return true,
            '[' if chars[index + 1..]
                .iter()
                .skip(1)
                .any(|&character| character == ']') =>
            {
                return true;
            }
            _ => {}
        }
        index += 1;
    }
    false
}

pub fn unescape(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
        } else {
            out.push(character);
        }
    }
    out
}

pub fn matches_pattern(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    let (mut pattern_index, mut text_index) = (0, 0);
    let mut star: Option<(usize, usize)> = None;

    while text_index < text_chars.len() {
        if pattern_index < pattern_chars.len() {
            match pattern_chars[pattern_index] {
                '*' => {
                    star = Some((pattern_index, text_index));
                    pattern_index += 1;
                    continue;
                }
                '?' => {
                    pattern_index += 1;
                    text_index += 1;
                    continue;
                }
                '[' => match bracket(&pattern_chars, pattern_index, text_chars[text_index]) {
                    Some((true, next)) => {
                        pattern_index = next;
                        text_index += 1;
                        continue;
                    }
                    Some((false, _)) => {}
                    None if text_chars[text_index] == '[' => {
                        pattern_index += 1;
                        text_index += 1;
                        continue;
                    }
                    None => {}
                },
                '\\' if pattern_index + 1 < pattern_chars.len() => {
                    if pattern_chars[pattern_index + 1] == text_chars[text_index] {
                        pattern_index += 2;
                        text_index += 1;
                        continue;
                    }
                }
                character if character == text_chars[text_index] => {
                    pattern_index += 1;
                    text_index += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some((star_pattern_index, star_text_index)) => {
                pattern_index = star_pattern_index + 1;
                text_index = star_text_index + 1;
                star = Some((star_pattern_index, star_text_index + 1));
            }
            None => return false,
        }
    }

    while pattern_chars.get(pattern_index) == Some(&'*') {
        pattern_index += 1;
    }
    pattern_index == pattern_chars.len()
}

fn bracket(pattern_chars: &[char], start: usize, target: char) -> Option<(bool, usize)> {
    let mut index = start + 1;
    let negate = matches!(pattern_chars.get(index), Some('!' | '^'));
    if negate {
        index += 1;
    }
    let mut matched = false;
    let mut first = true;
    while index < pattern_chars.len() {
        let mut member = pattern_chars[index];
        if member == ']' && !first {
            return Some((matched != negate, index + 1));
        }
        first = false;
        if member == '\\' && index + 1 < pattern_chars.len() {
            index += 1;
            member = pattern_chars[index];
        }
        if index + 2 < pattern_chars.len()
            && pattern_chars[index + 1] == '-'
            && pattern_chars[index + 2] != ']'
        {
            if member <= target && target <= pattern_chars[index + 2] {
                matched = true;
            }
            index += 3;
        } else {
            if member == target {
                matched = true;
            }
            index += 1;
        }
    }
    None
}

fn boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect()
}

pub fn remove_prefix(text: &str, pattern: &str, longest: bool) -> String {
    let cuts = boundaries(text);
    let mut candidates: Box<dyn Iterator<Item = &usize>> = if longest {
        Box::new(cuts.iter().rev())
    } else {
        Box::new(cuts.iter())
    };
    match candidates.find(|&&cut| matches_pattern(pattern, &text[..cut])) {
        Some(&cut) => text[cut..].to_string(),
        None => text.to_string(),
    }
}

pub fn remove_suffix(text: &str, pattern: &str, longest: bool) -> String {
    let cuts = boundaries(text);
    let mut candidates: Box<dyn Iterator<Item = &usize>> = if longest {
        Box::new(cuts.iter())
    } else {
        Box::new(cuts.iter().rev())
    };
    match candidates.find(|&&cut| matches_pattern(pattern, &text[cut..])) {
        Some(&cut) => text[..cut].to_string(),
        None => text.to_string(),
    }
}

pub fn replace(text: &str, pattern: &str, replacement: &str, all: bool, anchor: Anchor) -> String {
    if pattern.is_empty() {
        return text.to_string();
    }
    let cuts = boundaries(text);
    let mut out = String::new();
    let mut position = 0;
    while position < cuts.len() {
        let start = cuts[position];
        if anchor == Anchor::Start && start != 0 {
            break;
        }
        let found = cuts[position..].iter().rev().find(|&&end| {
            (anchor != Anchor::End || end == text.len())
                && matches_pattern(pattern, &text[start..end])
        });
        match found {
            Some(&end) if end > start => {
                out.push_str(replacement);
                position = cuts
                    .iter()
                    .position(|&cut| cut == end)
                    .unwrap_or(cuts.len());
                if !all {
                    out.push_str(&text[end..]);
                    return out;
                }
            }
            _ => {
                if let Some(&next) = cuts.get(position + 1) {
                    out.push_str(&text[start..next]);
                }
                position += 1;
            }
        }
    }
    if position < cuts.len() {
        out.push_str(&text[cuts[position]..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_matches_everything() {
        assert!(matches_pattern("*", "anything"));
        assert!(matches_pattern("*", ""));
        assert!(matches_pattern("*", "a"));
    }

    #[test]
    fn star_with_suffix() {
        assert!(matches_pattern("*.rs", "main.rs"));
        assert!(matches_pattern("*.rs", ".rs"));
        assert!(!matches_pattern("*.rs", "main.txt"));
        assert!(!matches_pattern("*.rs", "rs"));
    }

    #[test]
    fn star_with_prefix() {
        assert!(matches_pattern("main.*", "main.rs"));
        assert!(matches_pattern("main.*", "main."));
        assert!(!matches_pattern("main.*", "lib.rs"));
    }

    #[test]
    fn question_mark() {
        assert!(matches_pattern("?.rs", "a.rs"));
        assert!(!matches_pattern("?.rs", "ab.rs"));
        assert!(!matches_pattern("?.rs", ".rs"));
    }

    #[test]
    fn multiple_stars() {
        assert!(matches_pattern("*.*", "main.rs"));
        assert!(matches_pattern("*.*", "a.b"));
        assert!(!matches_pattern("*.*", "noext"));
    }

    #[test]
    fn exact_match() {
        assert!(matches_pattern("hello", "hello"));
        assert!(!matches_pattern("hello", "world"));
        assert!(!matches_pattern("hello", "hell"));
    }

    #[test]
    fn mixed_wildcards() {
        assert!(matches_pattern("t?st*.rs", "test_foo.rs"));
        assert!(matches_pattern("t?st*.rs", "tast.rs"));
        assert!(!matches_pattern("t?st*.rs", "toast.rs"));
    }

    #[test]
    fn expand_returns_none_for_no_matches() {
        assert!(expand("/nonexistent_dir_12345/*.xyz", GlobOptions::default()).is_none());
    }

    #[test]
    fn expand_src_glob() {
        let results =
            expand("src/*.rs", GlobOptions::default()).expect("should match .rs files in src/");
        assert!(results.iter().any(|file| file == "src/main.rs"));
        assert!(results.iter().any(|file| file == "src/shell.rs"));
        for file in &results {
            assert!(file.starts_with("src/"));
            assert!(file.ends_with(".rs"));
        }
    }

    #[test]
    fn expand_skips_hidden_files() {
        let results = expand("*", GlobOptions::default());
        if let Some(files) = results {
            assert!(files.iter().all(|file| !file.starts_with('.')));
        }
    }

    #[test]
    fn expand_results_are_sorted() {
        if let Some(results) = expand("src/*.rs", GlobOptions::default()) {
            let mut sorted = results.clone();
            sorted.sort();
            assert_eq!(results, sorted);
        }
    }

    #[test]
    fn globstar_and_case_options() {
        let deep = GlobOptions {
            globstar: true,
            ..GlobOptions::default()
        };
        let results = expand("src/**/*.rs", deep).unwrap();
        assert!(results.contains(&"src/lib.rs".to_string()));
        assert!(results.contains(&"src/table/filters.rs".to_string()));
        let flat = expand("src/**/*.rs", GlobOptions::default()).unwrap();
        assert!(!flat.contains(&"src/lib.rs".to_string()));
        let caseless = GlobOptions {
            nocase: true,
            ..GlobOptions::default()
        };
        assert_eq!(
            expand("CARGO.*", caseless).unwrap(),
            ["Cargo.lock", "Cargo.toml"]
        );
    }

    #[test]
    fn lone_brackets_are_not_patterns() {
        assert!(!has_magic("["));
        assert!(!has_magic("]"));
        assert!(!has_magic("[]"));
        assert!(has_magic("[ab]"));
        assert!(has_magic("x[]]"));
        assert!(!has_magic("\\*"));
    }

    #[test]
    fn bracket_classes() {
        assert!(matches_pattern("[abc].rs", "b.rs"));
        assert!(matches_pattern("[a-c]x", "cx"));
        assert!(!matches_pattern("[!a-c]x", "bx"));
        assert!(matches_pattern("[!a-c]x", "dx"));
        assert!(matches_pattern("[x", "[x"));
    }

    #[test]
    fn escaped_wildcards_are_literal() {
        assert!(matches_pattern("\\*.rs", "*.rs"));
        assert!(!matches_pattern("\\*.rs", "main.rs"));
    }

    #[test]
    fn multi_component_patterns() {
        let results = expand("s*/lib.rs", GlobOptions::default()).expect("src/lib.rs should match");
        assert_eq!(results, ["src/lib.rs"]);
    }

    #[test]
    fn prefix_and_suffix_removal() {
        assert_eq!(
            remove_prefix("src/expand/glob.rs", "*/", false),
            "expand/glob.rs"
        );
        assert_eq!(remove_prefix("src/expand/glob.rs", "*/", true), "glob.rs");
        assert_eq!(remove_suffix("archive.tar.gz", ".*", false), "archive.tar");
        assert_eq!(remove_suffix("archive.tar.gz", ".*", true), "archive");
        assert_eq!(remove_suffix("plain", "x*", true), "plain");
    }

    #[test]
    fn replacement() {
        assert_eq!(replace("a-b-c", "-", "+", false, Anchor::Anywhere), "a+b-c");
        assert_eq!(replace("a-b-c", "-", "+", true, Anchor::Anywhere), "a+b+c");
        assert_eq!(
            replace("main.rs", "main", "lib", false, Anchor::Start),
            "lib.rs"
        );
        assert_eq!(
            replace("main.rs", "rs", "go", false, Anchor::End),
            "main.go"
        );
        assert_eq!(replace("aaa", "a*", "x", false, Anchor::Anywhere), "x");
        assert_eq!(replace("hello", "l", "", true, Anchor::Anywhere), "heo");
    }
}
