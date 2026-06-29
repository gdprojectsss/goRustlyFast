use std::path::Path;
use std::process::{Command, Stdio};

use crate::commands;

const GIT_FALLBACK: &[&str] = &[
    "add",
    "bisect",
    "blame",
    "branch",
    "checkout",
    "cherry-pick",
    "clone",
    "commit",
    "config",
    "diff",
    "fetch",
    "grep",
    "init",
    "log",
    "merge",
    "mv",
    "pull",
    "push",
    "rebase",
    "remote",
    "reset",
    "restore",
    "revert",
    "rm",
    "show",
    "stash",
    "status",
    "switch",
    "tag",
    "worktree",
];

const CARGO_SUBCOMMANDS: &[&str] = &[
    "add",
    "bench",
    "build",
    "check",
    "clean",
    "clippy",
    "doc",
    "fetch",
    "fix",
    "fmt",
    "init",
    "install",
    "metadata",
    "new",
    "publish",
    "remove",
    "run",
    "search",
    "test",
    "tree",
    "uninstall",
    "update",
    "vendor",
];

fn lines_of(program: &str, args: &[&str]) -> Option<Vec<String>> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect(),
    )
}

pub(super) fn git_subcommands() -> Vec<String> {
    lines_of("git", &["--list-cmds=main,others,alias,nohelpers"])
        .unwrap_or_else(|| GIT_FALLBACK.iter().map(|name| name.to_string()).collect())
}

pub(super) fn git_refs() -> Vec<String> {
    lines_of(
        "git",
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )
    .unwrap_or_default()
}

pub(super) fn cargo_subcommands() -> Vec<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let installed = commands::on_path(&path)
        .into_iter()
        .filter_map(|name| name.strip_prefix("cargo-").map(String::from));
    CARGO_SUBCOMMANDS
        .iter()
        .map(|name| name.to_string())
        .chain(installed)
        .collect()
}

pub(super) fn ssh_hosts() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let ssh = Path::new(&home).join(".ssh");
    let mut hosts = Vec::new();
    if let Ok(config) = std::fs::read_to_string(ssh.join("config")) {
        hosts.extend(hosts_from_config(&config));
    }
    if let Ok(known) = std::fs::read_to_string(ssh.join("known_hosts")) {
        hosts.extend(hosts_from_known_hosts(&known));
    }
    hosts
}

fn hosts_from_config(config: &str) -> Vec<String> {
    config
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let (keyword, rest) = line.split_once(char::is_whitespace)?;
            keyword.eq_ignore_ascii_case("host").then_some(rest)
        })
        .flat_map(str::split_whitespace)
        .filter(|host| !host.contains(['*', '?', '!']))
        .map(String::from)
        .collect()
}

fn hosts_from_known_hosts(known: &str) -> Vec<String> {
    known
        .lines()
        .filter(|line| !line.starts_with('|') && !line.starts_with('#'))
        .filter_map(|line| line.split_whitespace().next())
        .flat_map(|field| field.split(','))
        .map(|host| match host.strip_prefix('[') {
            Some(bracketed) => bracketed.split(']').next().unwrap_or(bracketed),
            None => host,
        })
        .filter(|host| !host.is_empty())
        .map(String::from)
        .collect()
}

pub(super) fn make_targets() -> Vec<String> {
    ["GNUmakefile", "makefile", "Makefile"]
        .iter()
        .find_map(|name| std::fs::read_to_string(name).ok())
        .map(|makefile| targets_from_makefile(&makefile))
        .unwrap_or_default()
}

fn targets_from_makefile(makefile: &str) -> Vec<String> {
    makefile
        .lines()
        .filter(|line| !line.starts_with(['\t', ' ', '#', '.']))
        .filter_map(|line| {
            let (targets, rest) = line.split_once(':')?;
            (!rest.starts_with('=')).then_some(targets)
        })
        .flat_map(str::split_whitespace)
        .filter(|target| !target.contains(['$', '%', '=']))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_config_hosts_skip_patterns() {
        let config = "Host work-box db\n  HostName 10.0.0.2\nHost *.internal\nhost lowercase\n";
        assert_eq!(hosts_from_config(config), ["work-box", "db", "lowercase"]);
    }

    #[test]
    fn known_hosts_skip_hashed_entries() {
        let known = "github.com,140.82.1.1 ssh-ed25519 AAAA\n|1|hashed= ssh-rsa AAAA\n[box.local]:2222 ssh-rsa AAAA\n";
        assert_eq!(
            hosts_from_known_hosts(known),
            ["github.com", "140.82.1.1", "box.local"]
        );
    }

    #[test]
    fn makefile_targets() {
        let makefile =
            "CC := gcc\nall: build\nbuild test: deps\n\tcc main.c\n.PHONY: all\n%.o: %.c\n";
        assert_eq!(targets_from_makefile(makefile), ["all", "build", "test"]);
    }
}
