use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_TOTAL_RANK: f64 = 10_000.0;
const AGING_FACTOR: f64 = 0.9;
const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    path: String,
    rank: f64,
    last_visit: u64,
}

#[derive(Debug, Default)]
pub(crate) struct DirectoryHistory {
    file: Option<PathBuf>,
    entries: Vec<Entry>,
    loaded: bool,
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn frecency(entry: &Entry, now: u64) -> f64 {
    let age = now.saturating_sub(entry.last_visit);
    let weight = match age {
        0..HOUR => 4.0,
        HOUR..DAY => 2.0,
        DAY..WEEK => 0.5,
        _ => 0.25,
    };
    entry.rank * weight
}

fn matches(path: &str, terms: &[String]) -> bool {
    let path = path.to_lowercase();
    let last_component_start = path.rfind('/').map_or(0, |index| index + 1);
    let mut position = 0;
    for (index, term) in terms.iter().enumerate() {
        let term = term.to_lowercase();
        let Some(found) = path[position..].find(&term) else {
            return false;
        };
        position += found + term.len();
        let is_last = index + 1 == terms.len();
        if is_last && position <= last_component_start {
            return path[last_component_start..].contains(&term);
        }
    }
    true
}

impl DirectoryHistory {
    pub(crate) fn new(file: Option<PathBuf>) -> Self {
        DirectoryHistory {
            file,
            entries: Vec::new(),
            loaded: false,
        }
    }

    fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let Some(contents) = self
            .file
            .as_ref()
            .and_then(|file| std::fs::read_to_string(file).ok())
        else {
            return;
        };
        self.entries = contents
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(3, '\t');
                let rank = fields.next()?.parse().ok()?;
                let last_visit = fields.next()?.parse().ok()?;
                let path = fields.next()?.to_string();
                Some(Entry {
                    path,
                    rank,
                    last_visit,
                })
            })
            .collect();
    }

    fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        let contents: String = self
            .entries
            .iter()
            .map(|entry| format!("{}\t{}\t{}\n", entry.rank, entry.last_visit, entry.path))
            .collect();
        let temporary = file.with_extension("tmp");
        if crate::sys::write_private(&temporary, contents.as_bytes()).is_ok() {
            let _ = std::fs::rename(&temporary, file);
        }
    }

    pub(crate) fn visit(&mut self, path: &str, now: u64) {
        self.load();
        match self.entries.iter_mut().find(|entry| entry.path == path) {
            Some(entry) => {
                entry.rank += 1.0;
                entry.last_visit = now;
            }
            None => self.entries.push(Entry {
                path: path.to_string(),
                rank: 1.0,
                last_visit: now,
            }),
        }
        let total: f64 = self.entries.iter().map(|entry| entry.rank).sum();
        if total > MAX_TOTAL_RANK {
            for entry in &mut self.entries {
                entry.rank *= AGING_FACTOR;
            }
            self.entries.retain(|entry| entry.rank >= 1.0);
        }
        self.save();
    }

    pub(crate) fn ranked(&mut self, terms: &[String], now: u64) -> Vec<(f64, String)> {
        self.load();
        let mut ranked: Vec<(f64, String)> = self
            .entries
            .iter()
            .filter(|entry| matches(&entry.path, terms))
            .filter(|entry| Path::new(&entry.path).is_dir())
            .map(|entry| (frecency(entry, now), entry.path.clone()))
            .collect();
        ranked.sort_by(|left, right| right.0.total_cmp(&left.0));
        ranked
    }

    pub(crate) fn best_match(
        &mut self,
        terms: &[String],
        now: u64,
        exclude: &str,
    ) -> Option<String> {
        self.ranked(terms, now)
            .into_iter()
            .map(|(_, path)| path)
            .find(|path| path != exclude)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn terms_match_in_order_and_last_one_in_the_final_component() {
        assert!(matches("/home/me/projects/grfsh", &terms(&["proj", "grf"])));
        assert!(matches("/home/me/projects/grfsh", &terms(&["GRF"])));
        assert!(!matches(
            "/home/me/projects/grfsh",
            &terms(&["grf", "proj"])
        ));
        assert!(!matches("/home/me/projects/grfsh", &terms(&["me"])));
    }

    #[test]
    fn recent_visits_weigh_more() {
        let now = 10 * WEEK;
        let fresh = Entry {
            path: "/a".into(),
            rank: 2.0,
            last_visit: now - 60,
        };
        let stale = Entry {
            path: "/b".into(),
            rank: 6.0,
            last_visit: now - 2 * WEEK,
        };
        assert!(frecency(&fresh, now) > frecency(&stale, now));
    }

    #[test]
    fn visits_persist_and_rank() {
        let file = std::env::temp_dir().join(format!("grfsh_dirs_test_{}", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let directory = std::env::temp_dir()
            .to_string_lossy()
            .trim_end_matches('/')
            .to_string();

        let mut history = DirectoryHistory::new(Some(file.clone()));
        history.visit("/", 100);
        history.visit(&directory, 100);
        history.visit(&directory, 100);

        let mut reloaded = DirectoryHistory::new(Some(file.clone()));
        let ranked = reloaded.ranked(&[], 100);
        assert_eq!(
            ranked.first().map(|(_, path)| path.as_str()),
            Some(directory.as_str())
        );
        assert_eq!(
            reloaded.best_match(&[], 100, &directory).as_deref(),
            Some("/")
        );
        let _ = std::fs::remove_file(&file);
    }
}
