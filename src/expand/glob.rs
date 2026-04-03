pub fn expand(pattern: &str) -> Option<Vec<String>> {
    let mut paths = vec![if pattern.starts_with('/') {
        "/".to_string()
    } else {
        String::new()
    }];

    for component in pattern.split('/').filter(|part| !part.is_empty()) {
        let mut next = Vec::new();
        if !has_magic(component) {
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
                    if name.starts_with('.') && !component.starts_with('.') {
                        continue;
                    }
                    if matches_pattern(component, &name) {
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
        std::fs::symlink_metadata(path.trim_end_matches('/')).is_ok() || path == "/"
    });
    if paths.is_empty() {
        return None;
    }
    paths.sort();
    Some(paths)
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
    let mut chars = pattern.chars();
    while let Some(character) = chars.next() {
        match character {
            '\\' => {
                chars.next();
            }
            '*' | '?' | '[' => return true,
            _ => {}
        }
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
        assert!(expand("/nonexistent_dir_12345/*.xyz").is_none());
    }

    #[test]
    fn expand_src_glob() {
        let results = expand("src/*.rs").expect("should match .rs files in src/");
        assert!(results.iter().any(|file| file == "src/main.rs"));
        assert!(results.iter().any(|file| file == "src/shell.rs"));
        for file in &results {
            assert!(file.starts_with("src/"));
            assert!(file.ends_with(".rs"));
        }
    }

    #[test]
    fn expand_skips_hidden_files() {
        let results = expand("*");
        if let Some(files) = results {
            assert!(files.iter().all(|file| !file.starts_with('.')));
        }
    }

    #[test]
    fn expand_results_are_sorted() {
        if let Some(results) = expand("src/*.rs") {
            let mut sorted = results.clone();
            sorted.sort();
            assert_eq!(results, sorted);
        }
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
        let results = expand("s*/lib.rs").expect("src/lib.rs should match");
        assert_eq!(results, ["src/lib.rs"]);
    }
}
