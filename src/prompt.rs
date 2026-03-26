use std::env;
use std::fs;
use std::process::{Command, Stdio};

const GREEN: &str = "\x1b[1;32m";
const BLUE: &str = "\x1b[1;34m";
const MAGENTA: &str = "\x1b[1;35m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[33m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

pub fn build() -> String {
    match env::var("GRFSH_PROMPT") {
        Ok(fmt) => format_custom(&fmt),
        Err(_) => format_default(),
    }
}

fn format_custom(fmt: &str) -> String {
    let user = get_user();
    let host = get_host();
    let cwd = get_cwd();
    let branch = git_branch();
    let git = branch
        .as_ref()
        .map(|b| format!("({b}) "))
        .unwrap_or_default();

    fmt.replace("{user}", &user)
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

fn format_default() -> String {
    let user = get_user();
    let host = get_host();
    let cwd = get_cwd();

    match git_branch() {
        Some(b) => format!(
            "{GREEN}{user}@{host}{RESET}:{BLUE}{cwd}{RESET} {MAGENTA}({b}){RESET} $ "
        ),
        None => format!("{GREEN}{user}@{host}{RESET}:{BLUE}{cwd}{RESET} $ "),
    }
}

fn get_user() -> String {
    env::var("USER").unwrap_or_else(|_| "unknown".into())
}

fn get_host() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "localhost".into())
}

fn get_cwd() -> String {
    env::current_dir()
        .map(|p| shorten_home(&p.display().to_string()))
        .unwrap_or_else(|_| "?".into())
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
