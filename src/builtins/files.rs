use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::shell;
use crate::sys;

const REPLACE_USAGE: &str = "replace: usage: replace [--all] [-n] [-v] file old new, replace [--all] [-n] [-v] file < blocks, or search ... | replace [-n] old new";
const WRITE_USAGE: &str = "write-file: usage: write-file [-p] [-a] [-v] file < text";

fn target(path: &str) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path))
}

pub(crate) fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mode = std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode())
        .ok();
    let mut attempt = 0;
    let (temporary, mut file) = loop {
        let temporary = directory.join(format!(".{name}.grfsh-{}-{attempt}", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode.unwrap_or(0o666) & 0o7777)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt < 100 => {
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    };
    let written = file
        .write_all(contents)
        .and_then(|_| file.sync_all())
        .and_then(|_| match mode {
            Some(mode) => {
                std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(mode))
            }
            None => Ok(()),
        })
        .and_then(|_| std::fs::rename(&temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

fn read_stdin(command: &str) -> Result<String, String> {
    if sys::isatty(0) {
        return Err(format!("{command}: expects text on standard input"));
    }
    let mut text = String::new();
    std::io::stdin()
        .lock()
        .read_to_string(&mut text)
        .map_err(|error| format!("{command}: {error}"))?;
    Ok(text)
}

pub(crate) fn parse_blocks(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut blocks = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with("<<<<<<<") {
            return Err(format!(
                "replace: expected a line starting with <<<<<<<, found '{line}'"
            ));
        }
        let mut old = Vec::new();
        let mut new = Vec::new();
        let mut in_new = false;
        let mut closed = false;
        for line in lines.by_ref() {
            if !in_new && line.starts_with("=======") {
                in_new = true;
            } else if in_new && line.starts_with(">>>>>>>") {
                closed = true;
                break;
            } else if in_new {
                new.push(line);
            } else {
                old.push(line);
            }
        }
        if !closed {
            return Err("replace: a block is missing its ======= or >>>>>>> line".into());
        }
        blocks.push((old.join("\n"), new.join("\n")));
    }
    if blocks.is_empty() {
        return Err("replace: no <<<<<<< ======= >>>>>>> blocks on standard input".into());
    }
    Ok(blocks)
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

fn missing_hint(contents: &str, old: &str) -> String {
    let first = old
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if first.is_empty() {
        return String::new();
    }
    match contents.lines().position(|line| line.trim() == first) {
        Some(index) => format!(
            ", its first line appears at line {} but the rest differs",
            index + 1
        ),
        None => String::new(),
    }
}

pub(crate) fn apply_replacements(
    contents: &str,
    blocks: &[(String, String)],
    all: bool,
) -> Result<(String, usize), String> {
    let mut text = contents.to_string();
    let mut total = 0;
    for (old, new) in blocks {
        if old.is_empty() {
            return Err("the text to replace is empty".into());
        }
        let found: Vec<usize> = text
            .match_indices(old.as_str())
            .map(|(offset, _)| offset)
            .collect();
        match found.len() {
            0 => {
                let preview = old.lines().next().unwrap_or_default();
                return Err(format!(
                    "'{preview}' was not found{}",
                    missing_hint(&text, old)
                ));
            }
            1 => {}
            count if !all => {
                let lines: Vec<String> = found
                    .iter()
                    .map(|offset| line_of(&text, *offset).to_string())
                    .collect();
                return Err(format!(
                    "the text appears {count} times, at lines {}; include more surrounding text or use --all",
                    lines.join(", ")
                ));
            }
            _ => {}
        }
        total += found.len();
        text = text.replace(old.as_str(), new);
    }
    Ok((text, total))
}

pub(crate) fn changed_region(
    before: &str,
    after: &str,
) -> Option<(usize, Vec<String>, Vec<String>)> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let removed: Vec<String> = old[prefix..old.len() - suffix]
        .iter()
        .map(|line| line.to_string())
        .collect();
    let added: Vec<String> = new[prefix..new.len() - suffix]
        .iter()
        .map(|line| line.to_string())
        .collect();
    (!removed.is_empty() || !added.is_empty()).then_some((prefix + 1, removed, added))
}

fn show_change(out: &mut dyn Write, file: &str, before: &str, after: &str) {
    if let Some((line, removed, added)) = changed_region(before, after) {
        let _ = writeln!(out, "{file}:{line}");
        for text in removed {
            let _ = writeln!(out, "- {text}");
        }
        for text in added {
            let _ = writeln!(out, "+ {text}");
        }
    }
}

fn noun(count: usize) -> &'static str {
    if count == 1 {
        "replacement"
    } else {
        "replacements"
    }
}

fn replace_lines(contents: &str, lines: &[usize], old: &str, new: &str) -> (String, usize) {
    let mut count = 0;
    let updated = contents
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| {
            if lines.contains(&(index + 1)) {
                count += line.matches(old).count();
                line.replace(old, new)
            } else {
                line.to_string()
            }
        })
        .collect();
    (updated, count)
}

fn replace_piped(old: &str, new: &str, dry_run: bool, out: &mut dyn Write) -> i32 {
    if old.is_empty() {
        shell::error("replace: the text to replace is empty");
        return 2;
    }
    let input = match read_stdin("replace") {
        Ok(input) => input,
        Err(message) => {
            shell::error(message);
            return 2;
        }
    };
    let mut files: Vec<(String, Option<Vec<usize>>)> = Vec::new();
    let locations = match crate::table::locations(&input) {
        Ok(locations) => locations,
        Err(message) => {
            shell::error(format!("replace: {message}"));
            return 2;
        }
    };
    for (path, line) in locations {
        let position = files.iter().position(|(known, _)| *known == path);
        let index = position.unwrap_or_else(|| {
            files.push((path.clone(), Some(Vec::new())));
            files.len() - 1
        });
        match (line, &mut files[index].1) {
            (Some(line), Some(lines)) => lines.push(line),
            (None, scope) => *scope = None,
            _ => {}
        }
    }
    let mut total = 0;
    let mut status = 0;
    for (file, lines) in files {
        let path = target(&file);
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) => {
                shell::error(format!("replace: {file}: {}", sys::error_message(&error)));
                status = 1;
                continue;
            }
        };
        let (updated, count) = match lines {
            Some(lines) => replace_lines(&contents, &lines, old, new),
            None => (contents.replace(old, new), contents.matches(old).count()),
        };
        if count == 0 {
            continue;
        }
        total += count;
        if dry_run {
            show_change(out, &file, &contents, &updated);
            continue;
        }
        if let Err(error) = write_atomically(&path, updated.as_bytes()) {
            shell::error(format!("replace: {file}: {}", sys::error_message(&error)));
            status = 1;
            continue;
        }
        let _ = writeln!(out, "{file}: {count} {}", noun(count));
    }
    if total == 0 && status == 0 {
        shell::error(format!(
            "replace: '{old}' was not found in the piped locations"
        ));
        return 1;
    }
    status
}

pub(super) fn replace(args: &[String], out: &mut dyn Write) -> i32 {
    let mut all = false;
    let mut verbose = false;
    let mut dry_run = false;
    let mut positional = Vec::new();
    for argument in args {
        match argument.as_str() {
            "--all" | "-a" => all = true,
            "-v" | "--verbose" => verbose = true,
            "-n" | "--dry-run" => dry_run = true,
            _ => positional.push(argument.clone()),
        }
    }
    let blocks = match positional.as_slice() {
        [_, old, new] => vec![(old.clone(), new.clone())],
        [old, new] if !sys::isatty(0) => return replace_piped(old, new, dry_run, out),
        [_] => match read_stdin("replace").and_then(|text| parse_blocks(&text)) {
            Ok(blocks) => blocks,
            Err(message) => {
                shell::error(message);
                return 2;
            }
        },
        _ => {
            shell::error(REPLACE_USAGE);
            return 2;
        }
    };
    let file = &positional[0];
    let path = target(file);
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            shell::error(format!("replace: {file}: {}", sys::error_message(&error)));
            return 1;
        }
    };
    let (updated, count) = match apply_replacements(&contents, &blocks, all) {
        Ok(result) => result,
        Err(message) => {
            shell::error(format!("replace: {file}: {message}"));
            return 1;
        }
    };
    if dry_run {
        show_change(out, file, &contents, &updated);
        return 0;
    }
    if let Err(error) = write_atomically(&path, updated.as_bytes()) {
        shell::error(format!("replace: {file}: {}", sys::error_message(&error)));
        return 1;
    }
    if verbose {
        let _ = writeln!(out, "{file}: {count} {}", noun(count));
    }
    0
}

pub(super) fn write_file(args: &[String], out: &mut dyn Write) -> i32 {
    let mut parents = false;
    let mut append = false;
    let mut verbose = false;
    let mut files = Vec::new();
    for argument in args {
        match argument.as_str() {
            "-p" | "--parents" => parents = true,
            "-a" | "--append" => append = true,
            "-v" | "--verbose" => verbose = true,
            _ => files.push(argument),
        }
    }
    let [file] = files.as_slice() else {
        shell::error(WRITE_USAGE);
        return 2;
    };
    let text = match read_stdin("write-file") {
        Ok(text) => text,
        Err(message) => {
            shell::error(message);
            return 2;
        }
    };
    let path = target(file);
    if parents
        && let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        shell::error(format!(
            "write-file: {}: {}",
            parent.display(),
            sys::error_message(&error)
        ));
        return 1;
    }
    let result = if append {
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .and_then(|mut handle| handle.write_all(text.as_bytes()))
    } else {
        write_atomically(&path, text.as_bytes())
    };
    if let Err(error) = result {
        shell::error(format!(
            "write-file: {file}: {}",
            sys::error_message(&error)
        ));
        return 1;
    }
    if verbose {
        let _ = writeln!(
            out,
            "{file}: {} bytes, {} lines",
            text.len(),
            text.lines().count()
        );
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_replacement_must_be_unique() {
        let blocks = vec![("b".to_string(), "B".to_string())];
        assert_eq!(
            apply_replacements("abc", &blocks, false),
            Ok(("aBc".into(), 1))
        );
        let error = apply_replacements("b\nb\n", &blocks, false).unwrap_err();
        assert!(error.contains("2 times, at lines 1, 2"));
        assert_eq!(
            apply_replacements("b\nb\n", &blocks, true),
            Ok(("B\nB\n".into(), 2))
        );
    }

    #[test]
    fn missing_text_points_at_a_near_match() {
        let blocks = vec![("fn main() {\n    run();".to_string(), String::new())];
        let error = apply_replacements("x\nfn main() {\n  run();\n", &blocks, false).unwrap_err();
        assert!(error.contains("first line appears at line 2"));
    }

    #[test]
    fn changes_are_reported_as_one_region() {
        assert_eq!(
            changed_region("a\nb\nc\n", "a\nB\nc\n"),
            Some((2, vec!["b".into()], vec!["B".into()]))
        );
        assert_eq!(changed_region("same\n", "same\n"), None);
    }

    #[test]
    fn only_listed_lines_change() {
        assert_eq!(
            replace_lines("x\nx x\nx\n", &[2], "x", "y"),
            ("x\ny y\nx\n".into(), 2)
        );
    }

    #[test]
    fn blocks_are_parsed_in_order() {
        let text = "<<<<<<< old\na\nb\n=======\nc\n>>>>>>> new\n<<<<<<<\nx\n=======\n>>>>>>>\n";
        assert_eq!(
            parse_blocks(text),
            Ok(vec![
                ("a\nb".into(), "c".into()),
                ("x".into(), String::new())
            ])
        );
        assert!(parse_blocks("<<<<<<<\na\n").is_err());
        assert!(parse_blocks("junk").is_err());
    }
}
