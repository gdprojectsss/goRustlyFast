use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use crate::repl::git::GitInfo;

use super::{Icons, Theme};

pub(crate) const NAMES: &[&str] = &[
    "context", "dir", "git", "status", "duration", "jobs", "time", "venv", "rust", "node",
    "python", "go",
];

pub(in crate::repl) struct PromptContext<'a> {
    pub status: i32,
    pub duration: Duration,
    pub jobs: usize,
    pub git: Option<&'a GitInfo>,
    pub directory: &'a str,
}

pub(super) struct Segment {
    pub name: &'static str,
    pub text: String,
    pub color: &'static str,
}

fn icon(
    icons: Icons,
    nerd: &'static str,
    unicode: &'static str,
    ascii: &'static str,
) -> &'static str {
    match icons {
        Icons::Nerd => nerd,
        Icons::Unicode => unicode,
        Icons::Ascii => ascii,
    }
}

fn labelled(icon: &str, text: &str) -> String {
    if icon.is_empty() {
        text.to_string()
    } else {
        format!("{icon} {text}")
    }
}

fn directory_text(theme: &Theme, directory: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let shown = match directory.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => directory.to_string(),
    };
    let parts: Vec<&str> = shown.split('/').filter(|part| !part.is_empty()).collect();
    if theme.dir_depth == 0 || parts.len() <= theme.dir_depth {
        return shown;
    }
    let ellipsis = icon(theme.icons, "\u{2026}", "\u{2026}", "...");
    format!(
        "{ellipsis}/{}",
        parts[parts.len() - theme.dir_depth..].join("/")
    )
}

fn git_text(theme: &Theme, git: &GitInfo) -> String {
    let mut text = labelled(icon(theme.icons, "\u{e0a0}", "\u{2387}", ""), &git.branch);
    let markers = [
        (git.behind, icon(theme.icons, "\u{21e3}", "\u{21e3}", "v")),
        (git.ahead, icon(theme.icons, "\u{21e1}", "\u{21e1}", "^")),
        (git.staged, "+"),
        (git.unstaged, "!"),
        (git.untracked, "?"),
    ];
    for (count, marker) in markers {
        if count > 0 {
            text.push_str(&format!(" {marker}{count}"));
        }
    }
    text
}

fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    match seconds {
        0..60 => format!("{:.1}s", duration.as_secs_f64()),
        60..3600 => format!("{}m{:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h{:02}m", seconds / 3600, seconds % 3600 / 60),
    }
}

fn clock() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut broken_down: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut broken_down) };
    format!(
        "{:02}:{:02}:{:02}",
        broken_down.tm_hour, broken_down.tm_min, broken_down.tm_sec
    )
}

fn find_upwards(start: &str, markers: &[&str]) -> bool {
    let mut directory = Some(PathBuf::from(start));
    let home = std::env::var_os("HOME").map(PathBuf::from);
    for _ in 0..12 {
        let Some(current) = directory else {
            return false;
        };
        if markers.iter().any(|marker| current.join(marker).exists()) {
            return true;
        }
        if home.as_deref() == Some(current.as_path()) {
            return false;
        }
        directory = current.parent().map(Path::to_path_buf);
    }
    false
}

static VERSIONS: Mutex<Option<HashMap<&'static str, Option<String>>>> = Mutex::new(None);

fn tool_version(
    tool: &'static str,
    args: &[&str],
    extract: fn(&str) -> Option<String>,
) -> Option<String> {
    let mut cache = VERSIONS.lock().ok()?;
    let cache = cache.get_or_insert_with(HashMap::new);
    cache
        .entry(tool)
        .or_insert_with(|| {
            let output = Command::new(tool)
                .args(args)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()
                .filter(|output| output.status.success())?;
            extract(&String::from_utf8_lossy(&output.stdout))
        })
        .clone()
}

fn second_word(text: &str) -> Option<String> {
    text.split_whitespace().nth(1).map(String::from)
}

fn language(
    context: &PromptContext,
    theme: &Theme,
    markers: &[&str],
    tool: &'static str,
    args: &[&str],
    extract: fn(&str) -> Option<String>,
    icons: (&'static str, &'static str, &'static str),
) -> Option<String> {
    if !find_upwards(context.directory, markers) {
        return None;
    }
    let version = tool_version(tool, args, extract)?;
    Some(labelled(
        icon(theme.icons, icons.0, icons.1, icons.2),
        &version,
    ))
}

pub(super) fn segment(name: &str, theme: &Theme, context: &PromptContext) -> Option<Segment> {
    let (name, text, color): (&'static str, String, &'static str) = match name {
        "context" => {
            let remote = std::env::var_os("SSH_CONNECTION").is_some();
            let root = unsafe { libc::geteuid() } == 0;
            if !(remote || root || theme.context_always) {
                return None;
            }
            let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
            let host = crate::sys::hostname().unwrap_or_default();
            let host = host.split('.').next().unwrap_or_default().to_string();
            (
                "context",
                format!("{user}@{host}"),
                if root { "red" } else { "yellow" },
            )
        }
        "dir" => ("dir", directory_text(theme, context.directory), "blue"),
        "git" => (
            "git",
            git_text(theme, context.git?),
            if context.git?.dirty {
                "yellow"
            } else {
                "green"
            },
        ),
        "status" if context.status != 0 => {
            let mark = icon(theme.icons, "\u{2718}", "\u{2718}", "x");
            ("status", format!("{mark} {}", context.status), "red")
        }
        "duration" if context.duration.as_secs_f64() >= theme.duration_threshold => {
            let text = labelled(
                icon(theme.icons, "\u{f252}", "", "took"),
                &human_duration(context.duration),
            );
            ("duration", text, "yellow")
        }
        "jobs" if context.jobs > 0 => {
            let text = labelled(
                icon(theme.icons, "\u{f013}", "&", "jobs:"),
                &context.jobs.to_string(),
            );
            ("jobs", text.replace("jobs: ", "jobs:"), "cyan")
        }
        "time" => (
            "time",
            labelled(icon(theme.icons, "\u{f017}", "", ""), &clock()),
            "gray",
        ),
        "venv" => {
            let path = std::env::var("VIRTUAL_ENV")
                .or_else(|_| std::env::var("CONDA_DEFAULT_ENV"))
                .ok()?;
            let name = Path::new(&path).file_name()?.to_string_lossy().into_owned();
            (
                "venv",
                labelled(icon(theme.icons, "\u{e73c}", "py", "py"), &name),
                "cyan",
            )
        }
        "rust" => (
            "rust",
            language(
                context,
                theme,
                &["Cargo.toml"],
                "rustc",
                &["--version"],
                second_word,
                ("\u{e7a8}", "rs", "rs"),
            )?,
            "red",
        ),
        "node" => (
            "node",
            language(
                context,
                theme,
                &["package.json"],
                "node",
                &["--version"],
                |text| Some(text.trim().trim_start_matches('v').to_string()),
                ("\u{e718}", "node", "node"),
            )?,
            "green",
        ),
        "python" => (
            "python",
            language(
                context,
                theme,
                &["pyproject.toml", "requirements.txt", "setup.py"],
                "python3",
                &["--version"],
                second_word,
                ("\u{e73c}", "py", "py"),
            )?,
            "yellow",
        ),
        "go" => (
            "go",
            language(
                context,
                theme,
                &["go.mod"],
                "go",
                &["version"],
                |text| {
                    text.split_whitespace()
                        .nth(2)
                        .map(|version| version.trim_start_matches("go").to_string())
                },
                ("\u{e627}", "go", "go"),
            )?,
            "cyan",
        ),
        _ => return None,
    };
    Some(Segment { name, text, color })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(icons: Icons) -> Theme {
        Theme {
            icons,
            ..Theme::preset("lean").unwrap()
        }
    }

    #[test]
    fn long_directories_are_shortened() {
        let text = directory_text(&theme(Icons::Ascii), "/a/b/c/d/e");
        assert_eq!(text, ".../c/d/e");
        assert_eq!(directory_text(&theme(Icons::Ascii), "/a/b"), "/a/b");
    }

    #[test]
    fn git_markers() {
        let info = GitInfo {
            branch: "main".into(),
            dirty: true,
            staged: 1,
            unstaged: 2,
            untracked: 3,
            ahead: 1,
            behind: 0,
        };
        assert_eq!(git_text(&theme(Icons::Ascii), &info), "main ^1 +1 !2 ?3");
    }

    #[test]
    fn quiet_segments_hide_themselves() {
        let context = PromptContext {
            status: 0,
            duration: Duration::from_millis(100),
            jobs: 0,
            git: None,
            directory: "/",
        };
        let theme = theme(Icons::Ascii);
        for name in ["status", "duration", "jobs", "git"] {
            assert!(segment(name, &theme, &context).is_none(), "{name}");
        }
        assert!(segment("dir", &theme, &context).is_some());
    }

    #[test]
    fn durations() {
        assert_eq!(human_duration(Duration::from_millis(2500)), "2.5s");
        assert_eq!(human_duration(Duration::from_secs(125)), "2m05s");
    }
}
