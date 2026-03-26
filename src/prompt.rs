use std::env;
use std::fs;
use std::process::{Command, Stdio};

const GREEN: &str = "\x1b[1;32m";
const BLUE: &str = "\x1b[1;34m";
const MAGENTA: &str = "\x1b[1;35m";
const RESET: &str = "\x1b[0m";

pub fn build() -> String {
    let user = env::var("USER").unwrap_or_else(|_| "unknown".into());

    let host = fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "localhost".into());

    let cwd = env::current_dir()
        .map(|p| shorten_home(&p.display().to_string()))
        .unwrap_or_else(|_| "?".into());

    match git_branch() {
        Some(b) => format!(
            "{GREEN}{}@{}{RESET}:{BLUE}{}{RESET} {MAGENTA}({}){RESET} $ ",
            user, host, cwd, b
        ),
        None => format!(
            "{GREEN}{}@{}{RESET}:{BLUE}{}{RESET} $ ",
            user, host, cwd
        ),
    }
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
