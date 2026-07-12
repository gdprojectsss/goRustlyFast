use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;

pub(crate) fn on_path(path: &str) -> Vec<String> {
    let mut commands = BTreeSet::new();
    for directory in path.split(':').filter(|directory| !directory.is_empty()) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_file()
                && metadata.permissions().mode() & 0o111 != 0
                && let Ok(name) = entry.file_name().into_string()
            {
                commands.insert(name);
            }
        }
    }
    commands.into_iter().collect()
}

pub(crate) fn closest<'a>(
    name: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let limit = if name.chars().count() <= 4 { 1 } else { 2 };
    candidates
        .into_iter()
        .filter(|candidate| *candidate != name)
        .map(|candidate| (edit_distance(name, candidate), candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, candidate)| {
            (
                *distance,
                candidate.len().abs_diff(name.len()),
                !candidate.starts_with(&name[..name.len().min(1)]),
                *candidate,
            )
        })
        .map(|(_, candidate)| candidate)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let width = right.len() + 1;
    let mut table = vec![0usize; (left.len() + 1) * width];
    let at = |row: usize, column: usize| row * width + column;

    for row in 0..=left.len() {
        table[at(row, 0)] = row;
    }
    for column in 0..=right.len() {
        table[at(0, column)] = column;
    }
    for row in 1..=left.len() {
        for column in 1..=right.len() {
            let cost = usize::from(left[row - 1] != right[column - 1]);
            let mut best = (table[at(row - 1, column)] + 1)
                .min(table[at(row, column - 1)] + 1)
                .min(table[at(row - 1, column - 1)] + cost);
            if row > 1
                && column > 1
                && left[row - 1] == right[column - 2]
                && left[row - 2] == right[column - 1]
            {
                best = best.min(table[at(row - 2, column - 2)] + 1);
            }
            table[at(row, column)] = best;
        }
    }
    table[at(left.len(), right.len())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(edit_distance("git", "git"), 0);
        assert_eq!(edit_distance("gti", "git"), 1);
        assert_eq!(edit_distance("grpe", "grep"), 1);
        assert_eq!(edit_distance("cargo", "carog"), 1);
        assert_eq!(edit_distance("ls", "cat"), 3);
    }

    #[test]
    fn suggestions() {
        let commands = ["git", "grep", "cargo", "cat", "gzip"];
        assert_eq!(closest("gti", commands), Some("git"));
        assert_eq!(closest("grpe", commands), Some("grep"));
        assert_eq!(closest("carg", commands), Some("cargo"));
        assert_eq!(closest("kubectl", commands), None);
    }

    #[test]
    fn path_scan_finds_shell() {
        assert!(on_path("/bin").iter().any(|name| name == "sh"));
    }
}
