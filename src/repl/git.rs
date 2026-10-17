use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::signals;

const WAIT: Duration = Duration::from_millis(60);
const FRESH: Duration = Duration::from_millis(1500);
pub(super) const REDRAW_SIGNAL: libc::c_int = libc::SIGURG;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct GitInfo {
    pub branch: String,
    pub dirty: bool,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub ahead: usize,
    pub behind: usize,
}

struct Pending {
    directory: String,
    receiver: Receiver<Option<GitInfo>>,
    handle: JoinHandle<()>,
}

#[derive(Default)]
pub(super) struct GitPrompt {
    directory: String,
    info: Option<GitInfo>,
    updated: Option<Instant>,
    pending: Option<Pending>,
    redraw: Arc<AtomicBool>,
}

fn inside_repository(directory: &str) -> bool {
    if std::env::var_os("GIT_DIR").is_some() {
        return true;
    }
    Path::new(directory)
        .ancestors()
        .any(|ancestor| ancestor.join(".git").exists())
}

fn query(directory: &str) -> Option<GitInfo> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v2", "--branch"])
        .current_dir(directory)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    parse(&String::from_utf8_lossy(&output.stdout))
}

fn parse(status: &str) -> Option<GitInfo> {
    let mut head = None;
    let mut commit = None;
    let mut info = GitInfo::default();
    for line in status.lines() {
        if let Some(name) = line.strip_prefix("# branch.head ") {
            head = Some(name.to_string());
        } else if let Some(oid) = line.strip_prefix("# branch.oid ") {
            commit = Some(oid.chars().take(7).collect::<String>());
        } else if let Some(counts) = line.strip_prefix("# branch.ab ") {
            for count in counts.split_whitespace() {
                if let Some(ahead) = count.strip_prefix('+') {
                    info.ahead = ahead.parse().unwrap_or(0);
                } else if let Some(behind) = count.strip_prefix('-') {
                    info.behind = behind.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with("? ") {
            info.untracked += 1;
        } else if let Some(rest) = line
            .strip_prefix("1 ")
            .or_else(|| line.strip_prefix("2 "))
            .or_else(|| line.strip_prefix("u "))
        {
            let mut codes = rest.chars();
            if codes.next().is_some_and(|code| code != '.') {
                info.staged += 1;
            }
            if codes.next().is_some_and(|code| code != '.') {
                info.unstaged += 1;
            }
        }
    }
    info.dirty = info.staged + info.unstaged + info.untracked > 0;
    info.branch = match head?.as_str() {
        "(detached)" => commit.unwrap_or_else(|| "detached".into()),
        name => name.to_string(),
    };
    Some(info)
}

impl GitPrompt {
    fn collect(&mut self, wait: Duration) {
        let Some(pending) = &self.pending else {
            return;
        };
        let result = pending.receiver.recv_timeout(wait).ok();
        if let Some(info) = result {
            let pending = self.pending.take().expect("pending query");
            let _ = pending.handle.join();
            if pending.directory == self.directory {
                self.info = info;
                self.updated = Some(Instant::now());
            }
        }
    }

    fn spawn(&mut self, directory: &str) {
        let (sender, receiver) = mpsc::channel();
        let target = directory.to_string();
        let shown = self.info.clone();
        let redraw = Arc::clone(&self.redraw);
        let handle = thread::spawn(move || {
            let result = query(&target);
            let changed = result != shown;
            let _ = sender.send(result);
            if changed && redraw.load(Ordering::SeqCst) {
                signals::signal_main_thread(REDRAW_SIGNAL);
            }
        });
        self.pending = Some(Pending {
            directory: directory.to_string(),
            receiver,
            handle,
        });
    }

    pub(super) fn info(&mut self, directory: &str) -> Option<GitInfo> {
        if directory != self.directory {
            self.directory = directory.to_string();
            self.info = None;
            self.updated = None;
        }
        if !inside_repository(directory) {
            self.info = None;
            return None;
        }
        let fresh = self
            .updated
            .is_some_and(|updated| updated.elapsed() < FRESH);
        if !fresh && self.pending.is_none() {
            self.spawn(directory);
        }
        self.redraw.store(true, Ordering::SeqCst);
        self.collect(WAIT);
        if self.pending.is_none() {
            self.redraw.store(false, Ordering::SeqCst);
        }
        self.info.clone()
    }

    pub(super) fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn arrived(&mut self) -> bool {
        let before = self.info.clone();
        self.collect(Duration::ZERO);
        self.pending.is_none() && self.info != before
    }

    pub(super) fn stop_redraw(&self) {
        self.redraw.store(false, Ordering::SeqCst);
    }

    pub(super) fn invalidate(&mut self) {
        self.updated = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_branch() {
        let status = "# branch.oid 0123456789abcdef\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        let info = parse(status).unwrap();
        assert_eq!(info.branch, "main");
        assert!(!info.dirty);
    }

    #[test]
    fn dirty_and_detached() {
        let status = "# branch.oid 0123456789abcdef\n# branch.head (detached)\n1 .M N... 100644 100644 100644 a b src/lib.rs\n";
        let info = parse(status).unwrap();
        assert_eq!(info.branch, "0123456");
        assert!(info.dirty);
        assert_eq!(info.unstaged, 1);
    }

    #[test]
    fn counts_files_and_divergence() {
        let status = "# branch.head main\n# branch.ab +2 -1\n1 M. N... a b c d e staged.rs\n1 MM N... a b c d e both.rs\n? new.rs\n? other.rs\n";
        let info = parse(status).unwrap();
        assert_eq!((info.staged, info.unstaged, info.untracked), (2, 1, 2));
        assert_eq!((info.ahead, info.behind), (2, 1));
    }

    #[test]
    fn this_repository_has_a_branch() {
        let mut prompt = GitPrompt::default();
        let directory = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        prompt.info(&directory);
        prompt.stop_redraw();
        if prompt.waiting() {
            prompt.collect(Duration::from_secs(10));
        }
        assert!(
            prompt
                .info(&directory)
                .is_some_and(|info| !info.branch.is_empty())
        );
    }

    #[test]
    fn no_query_outside_a_repository() {
        let mut prompt = GitPrompt::default();
        assert_eq!(prompt.info("/"), None);
        assert!(!prompt.waiting());
    }

    #[test]
    fn invalidated_results_are_queried_again() {
        let mut prompt = GitPrompt::default();
        let directory = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        prompt.info(&directory);
        prompt.stop_redraw();
        prompt.collect(Duration::from_secs(10));
        assert!(prompt.updated.is_some());
        prompt.invalidate();
        assert!(prompt.updated.is_none());
        prompt.info(&directory);
        prompt.stop_redraw();
        assert!(prompt.waiting() || prompt.updated.is_some());
    }
}
