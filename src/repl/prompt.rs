use std::env;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::sys;

const GREEN: &str = "\x1b[1;32m";
const BLUE: &str = "\x1b[1;34m";
const MAGENTA: &str = "\x1b[1;35m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[33m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

const SLOW_COMMAND: Duration = Duration::from_secs(2);

pub struct PromptState {
    pub status: i32,
    pub duration: Duration,
    pub jobs: usize,
}

pub fn build(state: &PromptState) -> String {
    match env::var("GRFSH_PROMPT") {
        Ok(fmt) => format_custom(&fmt, state),
        Err(_) => format_default(state),
    }
}

fn status_text(state: &PromptState) -> String {
    if state.status == 0 {
        String::new()
    } else {
        state.status.to_string()
    }
}

fn duration_text(state: &PromptState) -> String {
    if state.duration < SLOW_COMMAND {
        return String::new();
    }
    let seconds = state.duration.as_secs();
    match seconds {
        0..60 => format!("{:.1}s", state.duration.as_secs_f64()),
        60..3600 => format!("{}m{:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h{:02}m", seconds / 3600, seconds % 3600 / 60),
    }
}

fn jobs_text(state: &PromptState) -> String {
    if state.jobs == 0 {
        String::new()
    } else {
        state.jobs.to_string()
    }
}

fn format_custom(fmt: &str, state: &PromptState) -> String {
    let user = get_user();
    let host = get_host();
    let cwd = get_cwd();
    let branch = git_branch();
    let git = branch
        .as_ref()
        .map(|b| format!("({b}) "))
        .unwrap_or_default();

    fmt.replace("{status}", &status_text(state))
        .replace("{duration}", &duration_text(state))
        .replace("{jobs}", &jobs_text(state))
        .replace("{user}", &user)
        .replace("{host}", &host)
        .replace("{cwd}", &cwd)
        .replace("{branch}", branch.as_deref().unwrap_or(""))
        .replace("{git}", &git)
        .replace("{green}", GREEN)
        .replace("{blue}", BLUE)
        .replace("{magenta}", MAGENTA)
        .replace("{cyan}", CYAN)
        .replace("{red}", RED)
        .replace("{yellow}", YELLOW)
        .replace("{bold}", BOLD)
        .replace("{reset}", RESET)
}

fn format_default(state: &PromptState) -> String {
    let user = get_user();
    let host = get_host();
    let cwd = get_cwd();

    let mut prompt = format!("{GREEN}{user}@{host}{RESET}:{BLUE}{cwd}{RESET}");
    if let Some(branch) = git_branch() {
        prompt.push_str(&format!(" {MAGENTA}({branch}){RESET}"));
    }
    let duration = duration_text(state);
    if !duration.is_empty() {
        prompt.push_str(&format!(" {YELLOW}took {duration}{RESET}"));
    }
    let jobs = jobs_text(state);
    if !jobs.is_empty() {
        prompt.push_str(&format!(" {CYAN}jobs:{jobs}{RESET}"));
    }
    let status = status_text(state);
    if status.is_empty() {
        prompt.push_str(" $ ");
    } else {
        prompt.push_str(&format!(" {RED}[{status}]{RESET} $ "));
    }
    prompt
}

fn get_user() -> String {
    env::var("USER").unwrap_or_else(|_| "unknown".into())
}

fn get_host() -> String {
    let host = sys::hostname().unwrap_or_else(|| "localhost".into());
    match host.split_once('.') {
        Some((short, _)) => short.to_string(),
        None => host,
    }
}

fn get_cwd() -> String {
    env::var("PWD")
        .ok()
        .or_else(|| {
            env::current_dir()
                .ok()
                .map(|path| path.display().to_string())
        })
        .map(|path| shorten_home(&path))
        .unwrap_or_else(|| "?".into())
}

fn shorten_home(path: &str) -> String {
    match env::var("HOME") {
        Ok(home) if path.starts_with(&home) => format!("~{}", &path[home.len()..]),
        _ => path.to_string(),
    }
}

fn git_branch() -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(status: i32, millis: u64, jobs: usize) -> PromptState {
        PromptState {
            status,
            duration: Duration::from_millis(millis),
            jobs,
        }
    }

    #[test]
    fn quick_successful_commands_add_nothing() {
        let quiet = state(0, 300, 0);
        assert_eq!(status_text(&quiet), "");
        assert_eq!(duration_text(&quiet), "");
        assert_eq!(jobs_text(&quiet), "");
    }

    #[test]
    fn durations_are_humanized() {
        assert_eq!(duration_text(&state(0, 2_500, 0)), "2.5s");
        assert_eq!(duration_text(&state(0, 125_000, 0)), "2m05s");
        assert_eq!(duration_text(&state(0, 3_720_000, 0)), "1h02m");
    }

    #[test]
    fn custom_format_placeholders() {
        let text = format_custom("{status}|{jobs}|{duration}", &state(127, 4_000, 2));
        assert_eq!(text, "127|2|4.0s");
    }
}
