use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_millis(60);
const FRESH: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct GitInfo {
    pub branch: String,
    pub dirty: bool,
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
    let mut dirty = false;
    for line in status.lines() {
        if let Some(name) = line.strip_prefix("# branch.head ") {
            head = Some(name.to_string());
        } else if let Some(oid) = line.strip_prefix("# branch.oid ") {
            commit = Some(oid.chars().take(7).collect::<String>());
        } else if !line.starts_with('#') && !line.is_empty() {
            dirty = true;
        }
    }
    let branch = match head?.as_str() {
        "(detached)" => commit.unwrap_or_else(|| "detached".into()),
        name => name.to_string(),
    };
    Some(GitInfo { branch, dirty })
}

impl GitPrompt {
    fn collect(&mut self, block: bool) {
        let Some(pending) = &self.pending else {
            return;
        };
        let result = if block {
            pending.receiver.recv().ok()
        } else {
            pending.receiver.recv_timeout(WAIT).ok()
        };
        if let Some(info) = result {
            let pending = self.pending.take().expect("pending query");
            let _ = pending.handle.join();
            if pending.directory == self.directory {
                self.info = info;
                self.updated = Some(Instant::now());
            }
        }
    }

    pub(super) fn info(&mut self, directory: &str) -> Option<GitInfo> {
        if directory != self.directory {
            self.directory = directory.to_string();
            self.info = None;
            self.updated = None;
        }
        let fresh = self
            .updated
            .is_some_and(|updated| updated.elapsed() < FRESH);
        if !fresh && self.pending.is_none() {
            let (sender, receiver) = mpsc::channel();
            let target = directory.to_string();
            let handle = thread::spawn(move || {
                let _ = sender.send(query(&target));
            });
            self.pending = Some(Pending {
                directory: directory.to_string(),
                receiver,
                handle,
            });
        }
        self.collect(false);
        self.info.clone()
    }

    pub(super) fn settle(&mut self) {
        self.collect(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_branch() {
        let status = "# branch.oid 0123456789abcdef\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        assert_eq!(
            parse(status),
            Some(GitInfo {
                branch: "main".into(),
                dirty: false
            })
        );
    }

    #[test]
    fn dirty_and_detached() {
        let status = "# branch.oid 0123456789abcdef\n# branch.head (detached)\n1 .M N... 100644 100644 100644 a b src/lib.rs\n";
        assert_eq!(
            parse(status),
            Some(GitInfo {
                branch: "0123456".into(),
                dirty: true
            })
        );
    }

    #[test]
    fn this_repository_has_a_branch() {
        let mut prompt = GitPrompt::default();
        let directory = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        prompt.info(&directory);
        prompt.settle();
        assert!(
            prompt
                .info(&directory)
                .is_some_and(|info| !info.branch.is_empty())
        );
    }
}
