use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::syntax::ast::Anchor;

mod locale;
mod pattern;

pub use pattern::Pattern;

static EXTGLOB: AtomicBool = AtomicBool::new(false);

pub fn extglob() -> bool {
    EXTGLOB.load(Ordering::Relaxed)
}

pub fn set_extglob(enabled: bool) {
    EXTGLOB.store(enabled, Ordering::Relaxed);
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobOptions {
    pub dotglob: bool,
    pub nocase: bool,
    pub globstar: bool,
    pub skipdots: bool,
    pub ignore: Vec<String>,
}

impl Default for GlobOptions {
    fn default() -> Self {
        GlobOptions {
            dotglob: false,
            nocase: false,
            globstar: false,
            skipdots: true,
            ignore: Vec::new(),
        }
    }
}

pub fn options(shell: &crate::shell::Shell) -> GlobOptions {
    let ignore: Vec<String> = shell
        .get_variable("GLOBIGNORE")
        .map(|value| ignore_patterns(&value))
        .unwrap_or_default();
    locale::configure(shell);
    GlobOptions {
        dotglob: shell.options.dotglob || !ignore.is_empty(),
        nocase: shell.options.nocaseglob,
        globstar: shell.options.globstar,
        skipdots: crate::builtins::shopt_enabled(shell, "globskipdots"),
        ignore,
    }
}

fn bracket_end(chars: &[char], start: usize) -> Option<usize> {
    let mut index = start + 1;
    if matches!(chars.get(index), Some('!' | '^')) {
        index += 1;
    }
    let first = index;
    while index < chars.len() {
        match chars[index] {
            ']' if index > first => return Some(index),
            '[' if chars.get(index + 1) == Some(&':') => {
                let close = chars[index + 2..]
                    .windows(2)
                    .position(|pair| pair == [':', ']'])?;
                index += close + 4;
                continue;
            }
            '\\' => index += 1,
            _ => {}
        }
        index += 1;
    }
    None
}

fn ignore_patterns(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut patterns = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            ':' => patterns.push(std::mem::take(&mut current)),
            '[' if let Some(end) = bracket_end(&chars, index) => {
                current.extend(&chars[index..=end]);
                index = end;
            }
            '\\' if index + 1 < chars.len() => {
                current.push('\\');
                current.push(chars[index + 1]);
                index += 1;
            }
            character => current.push(character),
        }
        index += 1;
    }
    patterns.push(current);
    patterns.retain(|pattern| !pattern.is_empty());
    patterns
}

pub fn interrupted() -> bool {
    crate::signals::is_pending(libc::SIGINT)
}

fn is_dot_entry(name: &str) -> bool {
    name == "." || name == ".."
}

fn visible(name: &str, component: &str, options: &GlobOptions) -> bool {
    if is_dot_entry(name) {
        return component.starts_with('.') && !options.skipdots;
    }
    !name.starts_with('.') || component.starts_with('.') || options.dotglob
}

fn descendants(base: &str, include_files: bool, options: &GlobOptions, out: &mut Vec<String>) {
    if interrupted() {
        return;
    }
    let directory = if base.is_empty() { "." } else { base };
    let Ok(entries) = std::fs::read_dir(crate::bytes::to_path(directory)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = crate::bytes::from_os(&entry.file_name());
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

fn directory_names(directory: &str) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(crate::bytes::to_path(directory)).ok()?;
    let mut names = vec![".".to_string(), "..".to_string()];
    for entry in entries.flatten() {
        if interrupted() {
            break;
        }
        names.push(crate::bytes::from_os(&entry.file_name()));
    }
    Some(names)
}

fn matching_entries(base: &str, component: &str, options: &GlobOptions, out: &mut Vec<String>) {
    let directory = if base.is_empty() { "." } else { base };
    let Some(names) = directory_names(directory) else {
        return;
    };
    for name in names {
        if visible(&name, component, options) && matches_with(component, &name, options.nocase) {
            out.push(join(base, &name));
        }
    }
}

fn path_matches(pattern: &str, path: &str, nocase: bool) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();
    pattern_parts.len() == path_parts.len()
        && pattern_parts
            .iter()
            .zip(&path_parts)
            .all(|(pattern, part)| matches_with(pattern, part, nocase))
}

fn acceptable(path: &str, options: &GlobOptions) -> bool {
    let terminal = match path.rfind('/') {
        Some(slash) if slash + 1 < path.len() => &path[slash + 1..],
        _ => path,
    };
    !is_dot_entry(terminal)
        && !options
            .ignore
            .iter()
            .any(|pattern| path_matches(pattern, path, options.nocase))
}

pub fn expand(pattern: &str, options: &GlobOptions) -> Option<Vec<String>> {
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
                } else if !base.is_empty() && base != "/" {
                    let base = base.trim_end_matches('/');
                    let marker = if pattern.ends_with('/') { "" } else { "/" };
                    next.push(format!("{base}{marker}"));
                }
                descendants(base, last, options, &mut next);
            }
        } else if !has_magic(component) {
            let name = unescape(component);
            next.extend(paths.iter().map(|base| join(base, &name)));
        } else {
            for base in &paths {
                matching_entries(base, component, options, &mut next);
            }
        }
        if interrupted() {
            return None;
        }
        paths = next;
    }

    if pattern.ends_with('/') {
        paths.retain(|path| crate::bytes::to_path(path).is_dir());
        paths.iter_mut().for_each(|path| path.push('/'));
    }
    paths.retain(|path| {
        !path.is_empty()
            && (std::fs::symlink_metadata(crate::bytes::to_path(path.trim_end_matches('/')))
                .is_ok()
                || path == "/")
    });
    if !options.ignore.is_empty() {
        paths.retain(|path| acceptable(path, options));
    }
    if paths.is_empty() {
        return None;
    }
    locale::sort(&mut paths);
    paths.dedup();
    Some(paths)
}

pub fn configure_locale(shell: &crate::shell::Shell) {
    locale::configure(shell);
}

fn byte_view(text: &str) -> Cow<'_, str> {
    if text.is_ascii() || !locale::byte_characters() {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(locale::to_bytes_view(text))
    }
}

fn in_byte_view<F: Fn(&str, &str) -> String>(text: &str, pattern: &str, operation: F) -> String {
    match byte_view(text) {
        Cow::Borrowed(_) => operation(text, pattern),
        Cow::Owned(viewed) => {
            locale::from_bytes_view(&operation(&viewed, &locale::to_bytes_view(pattern)))
        }
    }
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

fn extended_start(chars: &[char], index: usize) -> bool {
    matches!(chars[index], '@' | '+' | '!') && chars.get(index + 1) == Some(&'(')
}

pub fn has_magic(pattern: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    let extended = extglob();
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
            _ if extended && extended_start(&chars, index) => return true,
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
    Pattern::new(&byte_view(pattern), extglob()).matches(&byte_view(text))
}

pub fn matches_extended(pattern: &str, text: &str) -> bool {
    Pattern::new(&byte_view(pattern), true).matches(&byte_view(text))
}

pub fn remove_prefix(text: &str, pattern: &str, longest: bool) -> String {
    in_byte_view(text, pattern, |text, pattern| {
        prefix_removed(text, pattern, longest)
    })
}

pub fn remove_suffix(text: &str, pattern: &str, longest: bool) -> String {
    in_byte_view(text, pattern, |text, pattern| {
        suffix_removed(text, pattern, longest)
    })
}

fn prefix_removed(text: &str, pattern: &str, longest: bool) -> String {
    let compiled = Pattern::new(pattern, extglob());
    let chars: Vec<char> = text.chars().collect();
    match compiled.match_ends(&chars, 0, longest) {
        Some(end) => chars[end..].iter().collect(),
        None => text.to_string(),
    }
}

fn suffix_removed(text: &str, pattern: &str, longest: bool) -> String {
    let compiled = Pattern::new(pattern, extglob());
    let chars: Vec<char> = text.chars().collect();
    match compiled.suffix_start(&chars, longest) {
        Some(start) => chars[..start].iter().collect(),
        None => text.to_string(),
    }
}

fn replace_literal(
    text: &str,
    literal: &str,
    replacement: &str,
    all: bool,
    anchor: Anchor,
) -> String {
    match anchor {
        Anchor::Start => match text.strip_prefix(literal) {
            Some(rest) => format!("{replacement}{rest}"),
            None => text.to_string(),
        },
        Anchor::End => match text.strip_suffix(literal) {
            Some(rest) => format!("{rest}{replacement}"),
            None => text.to_string(),
        },
        Anchor::Anywhere if all => text.replace(literal, replacement),
        Anchor::Anywhere => text.replacen(literal, replacement, 1),
    }
}

fn replace_anywhere(chars: &[char], compiled: &Pattern, replacement: &str, all: bool) -> String {
    let first = compiled.first_char();
    let mut out = String::with_capacity(chars.len());
    let mut position = 0;
    while position < chars.len() {
        let candidate = first.is_none_or(|first| chars[position] == first);
        let end = if candidate {
            compiled.match_ends(chars, position, true)
        } else {
            None
        };
        match end {
            Some(end) if end > position => {
                out.push_str(replacement);
                position = end;
                if !all {
                    out.extend(&chars[position..]);
                    return out;
                }
            }
            _ => {
                out.push(chars[position]);
                position += 1;
            }
        }
    }
    out
}

pub fn replace(text: &str, pattern: &str, replacement: &str, all: bool, anchor: Anchor) -> String {
    in_byte_view(text, pattern, |text, pattern| {
        replaced(text, pattern, replacement, all, anchor)
    })
}

fn replaced(text: &str, pattern: &str, replacement: &str, all: bool, anchor: Anchor) -> String {
    let compiled = Pattern::new(pattern, extglob());
    if compiled.is_empty() && anchor == Anchor::Anywhere {
        return text.to_string();
    }
    if let Some(literal) = compiled.literal() {
        return replace_literal(text, &literal, replacement, all, anchor);
    }
    let chars: Vec<char> = text.chars().collect();
    match anchor {
        Anchor::Start => match compiled.match_ends(&chars, 0, true) {
            Some(end) => format!("{replacement}{}", chars[end..].iter().collect::<String>()),
            None => text.to_string(),
        },
        Anchor::End => match compiled.suffix_start(&chars, true) {
            Some(start) => format!("{}{replacement}", chars[..start].iter().collect::<String>()),
            None => text.to_string(),
        },
        Anchor::Anywhere => replace_anywhere(&chars, &compiled, replacement, all),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globignore_splits_on_colons_outside_brackets() {
        assert_eq!(ignore_patterns("*.o:*.h"), ["*.o", "*.h"]);
        assert_eq!(ignore_patterns("[[:alnum:]]*"), ["[[:alnum:]]*"]);
        assert_eq!(ignore_patterns("[:]x::y"), ["[:]x", "y"]);
        assert_eq!(ignore_patterns(".:.."), [".", ".."]);
    }

    #[test]
    fn globignore_matches_whole_paths() {
        let options = GlobOptions {
            ignore: vec!["*.txt".into()],
            ..GlobOptions::default()
        };
        assert!(!acceptable("one.txt", &options));
        assert!(acceptable("foo/two.txt", &options));
        assert!(!acceptable("foo/..", &options));
    }

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
        assert!(expand("/nonexistent_dir_12345/*.xyz", &GlobOptions::default()).is_none());
    }

    #[test]
    fn expand_src_glob() {
        let results =
            expand("src/*.rs", &GlobOptions::default()).expect("should match .rs files in src/");
        assert!(results.iter().any(|file| file == "src/main.rs"));
        assert!(results.iter().any(|file| file == "src/shell.rs"));
        for file in &results {
            assert!(file.starts_with("src/"));
            assert!(file.ends_with(".rs"));
        }
    }

    #[test]
    fn expand_skips_hidden_files() {
        let results = expand("*", &GlobOptions::default());
        if let Some(files) = results {
            assert!(files.iter().all(|file| !file.starts_with('.')));
        }
    }

    #[test]
    fn expand_results_are_sorted() {
        if let Some(results) = expand("src/*.rs", &GlobOptions::default()) {
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
        let results = expand("src/**/*.rs", &deep).unwrap();
        assert!(results.contains(&"src/lib.rs".to_string()));
        assert!(results.contains(&"src/table/filters.rs".to_string()));
        let flat = expand("src/**/*.rs", &GlobOptions::default()).unwrap();
        assert!(!flat.contains(&"src/lib.rs".to_string()));
        let caseless = GlobOptions {
            nocase: true,
            ..GlobOptions::default()
        };
        assert_eq!(
            expand("CARGO.*", &caseless).unwrap(),
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
    fn posix_character_classes() {
        assert!(matches_pattern("[[:digit:]]*", "42abc"));
        assert!(!matches_pattern("[[:digit:]]*", "abc"));
        assert!(matches_pattern("x[[:space:][:punct:]]y", "x y"));
        assert!(matches_pattern("x[[:space:][:punct:]]y", "x,y"));
        assert!(matches_pattern("[![:upper:]]", "a"));
    }

    #[test]
    fn escaped_wildcards_are_literal() {
        assert!(matches_pattern("\\*.rs", "*.rs"));
        assert!(!matches_pattern("\\*.rs", "main.rs"));
    }

    #[test]
    fn multi_component_patterns() {
        let results =
            expand("s*/lib.rs", &GlobOptions::default()).expect("src/lib.rs should match");
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
        assert_eq!(replace("abc", "", "X", false, Anchor::Start), "Xabc");
        assert_eq!(replace("abc", "", "X", false, Anchor::End), "abcX");
        assert_eq!(replace("aXbXc", "X*", "", false, Anchor::End), "a");
        assert_eq!(
            replace("aXbXc", "[a-c]", ".", true, Anchor::Anywhere),
            ".X.X."
        );
        assert_eq!(replace("hello", "h*l", "H", false, Anchor::Start), "Ho");
        assert_eq!(replace("hello", "l*o", "O", false, Anchor::End), "heO");
    }

    #[test]
    fn replacement_is_fast_on_long_text() {
        let text = "a".repeat(20_000);
        let started = std::time::Instant::now();
        assert_eq!(
            replace(&text, "a", "b", true, Anchor::Anywhere).len(),
            20_000
        );
        assert_eq!(
            replace(&text, "[ab]", "c", true, Anchor::Anywhere).len(),
            20_000
        );
        assert_eq!(
            replace(&text, "b", "c", true, Anchor::Anywhere).len(),
            20_000
        );
        assert_eq!(replace(&text, "a*a", "c", true, Anchor::Anywhere), "c");
        assert_eq!(remove_suffix(&text, "a*", false), text[..19_999]);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
