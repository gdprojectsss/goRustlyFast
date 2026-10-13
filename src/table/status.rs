use std::collections::BTreeSet;
use std::process::{Command, Stdio};

use super::Table;
use super::value::Value;

fn state_name(index: char, worktree: char) -> &'static str {
    match (index, worktree) {
        ('?', '?') => "untracked",
        ('!', '!') => "ignored",
        ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => "conflict",
        ('R', _) | (_, 'R') => "renamed",
        ('C', _) | (_, 'C') => "copied",
        ('A', _) => "added",
        ('D', _) | (_, 'D') => "deleted",
        ('T', _) | (_, 'T') => "typechange",
        _ => "modified",
    }
}

pub(super) fn parse_porcelain(text: &str) -> Table {
    let mut table = Table::new(vec![
        "path".into(),
        "state".into(),
        "staged".into(),
        "unstaged".into(),
        "from".into(),
    ]);
    let mut entries = text.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        let mut chars = entry.chars();
        let (Some(index), Some(worktree)) = (chars.next(), chars.next()) else {
            continue;
        };
        let path = entry.get(3..).unwrap_or_default().to_string();
        let state = state_name(index, worktree);
        let from = if matches!(index, 'R' | 'C') || matches!(worktree, 'R' | 'C') {
            entries
                .next()
                .map_or(Value::Nothing, |old| Value::Text(old.to_string()))
        } else {
            Value::Nothing
        };
        let untracked = state == "untracked";
        table.push(vec![
            Value::Text(path),
            Value::Text(state.into()),
            Value::Bool(!untracked && !matches!(index, ' ' | '?' | '!')),
            Value::Bool(untracked || !matches!(worktree, ' ' | '?' | '!')),
            from,
        ]);
    }
    table
}

pub(super) fn git_status(args: &[String]) -> Result<Table, String> {
    let mut command = Command::new("git");
    command.args(["status", "--porcelain=v1", "-z", "--untracked-files=all"]);
    if args.iter().any(|argument| argument == "--ignored") {
        command.arg("--ignored");
    }
    let paths: Vec<&String> = args
        .iter()
        .filter(|argument| !argument.starts_with('-'))
        .collect();
    if !paths.is_empty() {
        command.arg("--").args(paths);
    }
    let output = command
        .stdin(Stdio::null())
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(|error| format!("could not run git: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(message.trim().trim_start_matches("fatal: ").to_string());
    }
    Ok(parse_porcelain(&String::from_utf8_lossy(&output.stdout)))
}

fn port_table() -> Table {
    Table::new(vec![
        "protocol".into(),
        "address".into(),
        "port".into(),
        "pid".into(),
        "process".into(),
    ])
}

fn split_address(text: &str) -> Option<(String, u16)> {
    let (address, port) = text.rsplit_once(':')?;
    let address = address.trim_start_matches('[').trim_end_matches(']');
    Some((address.to_string(), port.parse().ok()?))
}

pub(super) fn parse_lsof(text: &str) -> Table {
    let mut rows = BTreeSet::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        let name_index = fields
            .iter()
            .position(|field| *field == "TCP" || *field == "UDP")
            .unwrap_or(fields.len() - 2);
        let Some(name) = fields.get(name_index + 1) else {
            continue;
        };
        let Some((address, port)) = split_address(name) else {
            continue;
        };
        let protocol = if fields[4] == "IPv6" { "tcp6" } else { "tcp" };
        let pid: i64 = fields[1].parse().unwrap_or(0);
        rows.insert((
            port,
            protocol.to_string(),
            address,
            pid,
            fields[0].replace("\\x20", " "),
        ));
    }
    let mut table = port_table();
    for (port, protocol, address, pid, process) in rows {
        table.push(vec![
            Value::Text(protocol),
            Value::Text(address),
            Value::Int(i64::from(port)),
            Value::Int(pid),
            Value::Text(process),
        ]);
    }
    table
}

fn hex_address(text: &str) -> Option<(String, u16)> {
    let (address, port) = text.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let address = match address.len() {
        8 => {
            let value = u32::from_str_radix(address, 16).ok()?;
            std::net::Ipv4Addr::from(u32::from_be(value)).to_string()
        }
        32 => {
            let mut bytes = [0u8; 16];
            for (chunk, slot) in address.as_bytes().chunks(8).zip(bytes.chunks_mut(4)) {
                let word = u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
                slot.copy_from_slice(&word.to_le_bytes());
            }
            std::net::Ipv6Addr::from(bytes).to_string()
        }
        _ => return None,
    };
    Some((address, port))
}

pub(super) fn parse_proc_net(text: &str, protocol: &str) -> Vec<(String, String, u16, u64)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.get(3) != Some(&"0A") {
                return None;
            }
            let (address, port) = hex_address(fields.get(1)?)?;
            let inode = fields.get(9)?.parse().ok()?;
            Some((protocol.to_string(), address, port, inode))
        })
        .collect()
}

fn socket_owners() -> std::collections::HashMap<u64, (i64, String)> {
    let mut owners = std::collections::HashMap::new();
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return owners;
    };
    for process in processes.flatten() {
        let Some(pid) = process
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i64>().ok())
        else {
            continue;
        };
        let Ok(descriptors) = std::fs::read_dir(process.path().join("fd")) else {
            continue;
        };
        let name = std::fs::read_to_string(process.path().join("comm"))
            .map(|name| name.trim().to_string())
            .unwrap_or_default();
        for descriptor in descriptors.flatten() {
            let Ok(target) = std::fs::read_link(descriptor.path()) else {
                continue;
            };
            let target = target.to_string_lossy();
            if let Some(inode) = target
                .strip_prefix("socket:[")
                .and_then(|rest| rest.strip_suffix(']'))
                .and_then(|inode| inode.parse().ok())
            {
                owners.entry(inode).or_insert((pid, name.clone()));
            }
        }
    }
    owners
}

pub(super) fn ports(args: &[String]) -> Result<Table, String> {
    if let Some(argument) = args.first() {
        return Err(format!("unexpected argument '{argument}'"));
    }
    if std::path::Path::new("/proc/net/tcp").exists() {
        let mut sockets = Vec::new();
        for (file, protocol) in [("/proc/net/tcp", "tcp"), ("/proc/net/tcp6", "tcp6")] {
            if let Ok(text) = std::fs::read_to_string(file) {
                sockets.extend(parse_proc_net(&text, protocol));
            }
        }
        let owners = socket_owners();
        let mut rows = BTreeSet::new();
        for (protocol, address, port, inode) in sockets {
            let (pid, process) = owners.get(&inode).cloned().unwrap_or((-1, String::new()));
            rows.insert((port, protocol, address, pid, process));
        }
        let mut table = port_table();
        for (port, protocol, address, pid, process) in rows {
            let known = pid >= 0;
            table.push(vec![
                Value::Text(protocol),
                Value::Text(address),
                Value::Int(i64::from(port)),
                if known {
                    Value::Int(pid)
                } else {
                    Value::Nothing
                },
                if known {
                    Value::Text(process)
                } else {
                    Value::Nothing
                },
            ]);
        }
        return Ok(table);
    }
    let output = Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("could not run lsof: {error}"))?;
    Ok(parse_lsof(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_entries_become_rows() {
        let table = parse_porcelain(" M src/a.rs\0A  new.rs\0R  b.rs\0old.rs\0?? notes.txt\0");
        let states: Vec<&Value> = table.rows.iter().map(|row| &row[1]).collect();
        let expected: Vec<Value> = ["modified", "added", "renamed", "untracked"]
            .iter()
            .map(|state| Value::Text(state.to_string()))
            .collect();
        assert_eq!(states, expected.iter().collect::<Vec<_>>());
        assert_eq!(table.rows[0][2], Value::Bool(false));
        assert_eq!(table.rows[0][3], Value::Bool(true));
        assert_eq!(table.rows[2][4], Value::Text("old.rs".into()));
    }

    #[test]
    fn lsof_listeners() {
        let text = "COMMAND   PID USER   FD   TYPE DEVICE SIZE/OFF NODE NAME\n\
                    postgres 812 me    7u  IPv4 0x1      0t0  TCP 127.0.0.1:5432 (LISTEN)\n\
                    node     900 me   21u  IPv6 0x2      0t0  TCP [::1]:3000 (LISTEN)\n";
        let table = parse_lsof(text);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0][2], Value::Int(3000));
        assert_eq!(table.rows[0][1], Value::Text("::1".into()));
        assert_eq!(table.rows[1][4], Value::Text("postgres".into()));
    }

    #[test]
    fn proc_net_listeners() {
        let text = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
                    0: 0100007F:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1\n\
                    1: 0100007F:9C40 0100007F:1538 01 00000000:00000000 00:00000000 00000000  1000        0 4343 1\n";
        assert_eq!(
            parse_proc_net(text, "tcp"),
            [("tcp".to_string(), "127.0.0.1".to_string(), 5432, 4242)]
        );
    }
}
