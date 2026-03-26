pub fn expand(pattern: &str) -> Option<Vec<String>> {
    let (dir, file_pattern) = match pattern.rfind('/') {
        Some(i) => (&pattern[..=i], &pattern[i + 1..]),
        None => ("", pattern),
    };

    let read_dir = if dir.is_empty() { "." } else { dir };
    let entries = std::fs::read_dir(read_dir).ok()?;

    let mut matches: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if name.starts_with('.') && !file_pattern.starts_with('.') {
                return None;
            }
            if matches_pattern(file_pattern, &name) {
                Some(format!("{dir}{name}"))
            } else {
                None
            }
        })
        .collect();

    if matches.is_empty() {
        return None;
    }

    matches.sort();
    Some(matches)
}

pub fn matches_pattern(pattern: &str, text: &str) -> bool {
    match pattern.find(['*', '?']) {
        None => pattern == text,
        Some(pos) => {
            let (prefix, rest) = pattern.split_at(pos);
            if !text.starts_with(prefix) {
                return false;
            }
            let remaining_text = &text[prefix.len()..];
            let wildcard = rest.as_bytes()[0];
            let after_wildcard = &rest[1..];

            match wildcard {
                b'?' => {
                    let mut chars = remaining_text.chars();
                    match chars.next() {
                        Some(_) => matches_pattern(after_wildcard, chars.as_str()),
                        None => false,
                    }
                }
                b'*' => {
                    for i in 0..=remaining_text.len() {
                        if matches_pattern(after_wildcard, &remaining_text[i..]) {
                            return true;
                        }
                    }
                    false
                }
                _ => unreachable!(),
            }
        }
    }
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
        assert!(results.iter().any(|f| f == "src/main.rs"));
        assert!(results.iter().any(|f| f == "src/parser.rs"));
        for f in &results {
            assert!(f.starts_with("src/"));
            assert!(f.ends_with(".rs"));
        }
    }

    #[test]
    fn expand_skips_hidden_files() {
        let results = expand("*");
        if let Some(files) = results {
            assert!(files.iter().all(|f| !f.starts_with('.')));
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
}
