use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;

const DEFAULT_LINES: usize = 400;
const LONGEST_LINE: usize = 2000;

#[derive(Debug, PartialEq)]
pub(crate) struct Trimmed {
    pub text: String,
    pub omitted_lines: usize,
}

pub(crate) fn line_limit() -> usize {
    std::env::var("GRFSH_OUTPUT_LINES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_LINES)
}

fn shorten(line: &str) -> String {
    match line.char_indices().nth(LONGEST_LINE) {
        Some((cut, _)) => {
            let rest = line[cut..].chars().count();
            format!("{} [{rest} more characters]", &line[..cut])
        }
        None => line.to_string(),
    }
}

pub(crate) fn trim(text: &str, limit: usize) -> Trimmed {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let long_line = lines.iter().any(|line| line.chars().count() > LONGEST_LINE);
    if limit == 0 || (lines.len() <= limit && !long_line) {
        return Trimmed {
            text: text.to_string(),
            omitted_lines: 0,
        };
    }
    let keep = |line: &&str| {
        let ending = if line.ends_with('\n') { "\n" } else { "" };
        shorten(line.trim_end_matches('\n')) + ending
    };
    if lines.len() <= limit {
        return Trimmed {
            text: lines.iter().map(keep).collect(),
            omitted_lines: 0,
        };
    }
    let head = limit / 2;
    let tail = limit - head;
    let omitted = lines.len() - head - tail;
    let mut out: String = lines[..head].iter().map(keep).collect();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!("[grfsh: {omitted} lines omitted]\n"));
    out.extend(lines[lines.len() - tail..].iter().map(keep));
    Trimmed {
        text: out,
        omitted_lines: omitted,
    }
}

const LOG_LIFETIME: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

fn directory_path() -> PathBuf {
    std::env::temp_dir().join(format!("grfsh-output-{}", unsafe { libc::getuid() }))
}

pub(crate) fn remove_old_logs() {
    let directory = directory_path();
    let Ok(metadata) = std::fs::symlink_metadata(&directory) else {
        return;
    };
    let owned = std::os::unix::fs::MetadataExt::uid(&metadata) == unsafe { libc::getuid() };
    if !metadata.is_dir() || !owned {
        return;
    }
    remove_files_older_than(&directory, LOG_LIFETIME);
}

fn remove_files_older_than(directory: &std::path::Path, lifetime: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let old = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > lifetime);
        if metadata.is_file() && old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub(crate) fn output_directory() -> Option<PathBuf> {
    let directory = directory_path();
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .ok()?;
    let metadata = std::fs::symlink_metadata(&directory).ok()?;
    let owned = std::os::unix::fs::MetadataExt::uid(&metadata) == unsafe { libc::getuid() };
    (metadata.is_dir() && owned).then_some(directory)
}

pub(crate) fn save_full_output(text: &str, label: &str) -> Option<PathBuf> {
    let directory = output_directory()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let path = directory.join(format!("{stamp}-{}-{label}.log", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .ok()?;
    file.write_all(&crate::bytes::encode(text)).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_logs_are_removed() {
        let directory = std::env::temp_dir().join(format!("grfsh_logs_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let log = directory.join("old.log");
        std::fs::write(&log, "x").unwrap();
        remove_files_older_than(&directory, std::time::Duration::from_secs(3600));
        assert!(log.exists());
        std::thread::sleep(std::time::Duration::from_millis(20));
        remove_files_older_than(&directory, std::time::Duration::from_millis(1));
        assert!(!log.exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn short_output_is_unchanged() {
        let trimmed = trim("a\nb\n", 10);
        assert_eq!(trimmed.text, "a\nb\n");
        assert_eq!(trimmed.omitted_lines, 0);
    }

    #[test]
    fn long_output_keeps_head_and_tail() {
        let text: String = (1..=10).map(|number| format!("{number}\n")).collect();
        let trimmed = trim(&text, 4);
        assert_eq!(trimmed.text, "1\n2\n[grfsh: 6 lines omitted]\n9\n10\n");
        assert_eq!(trimmed.omitted_lines, 6);
    }

    #[test]
    fn very_long_lines_are_shortened() {
        let line = "x".repeat(LONGEST_LINE + 5);
        let trimmed = trim(&line, 10);
        assert!(trimmed.text.ends_with("[5 more characters]"));
    }

    #[test]
    fn zero_disables_the_limit() {
        let text: String = (1..=10).map(|number| format!("{number}\n")).collect();
        assert_eq!(trim(&text, 0).text, text);
    }
}
