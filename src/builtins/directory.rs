use std::io::Write;
use std::path::{Component, PathBuf};

use crate::directories;
use crate::shell::{self, Shell};
use crate::sys;

fn logical_path(working_directory: &str, target: &str) -> PathBuf {
    let joined = if target.starts_with('/') {
        crate::bytes::to_path(target)
    } else {
        crate::bytes::to_path(working_directory).join(crate::bytes::to_path(target))
    };
    let mut out = PathBuf::from("/");
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
            _ => {}
        }
    }
    out
}

pub(super) fn cd(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let target = match args.first().map(String::as_str) {
        None => match shell.get_variable("HOME") {
            Some(home) => home,
            None => {
                shell::error("cd: HOME not set");
                return 1;
            }
        },
        Some("-") => match shell.get_variable("OLDPWD") {
            Some(prev) => {
                let _ = writeln!(out, "{prev}");
                prev
            }
            None => {
                shell::error("cd: OLDPWD not set");
                return 1;
            }
        },
        Some(directory) => directory.to_string(),
    };
    let searchable = !target.starts_with('/') && !target.starts_with('.');
    if searchable && let Some(cdpath) = shell.get_variable("CDPATH") {
        for base in cdpath.split(':').filter(|base| !base.is_empty()) {
            let candidate = crate::bytes::to_path(base).join(crate::bytes::to_path(&target));
            if candidate.is_dir() {
                let candidate = crate::bytes::from_path(&candidate);
                let status = change_directory(shell, &candidate);
                if status == 0 {
                    let _ = writeln!(out, "{}", working_directory(shell));
                }
                return status;
            }
        }
    }
    change_directory(shell, &target)
}

pub(crate) fn change_directory(shell: &mut Shell, target: &str) -> i32 {
    let previous = working_directory(shell);
    let logical = logical_path(&previous, target);
    let destination = if std::env::set_current_dir(&logical).is_ok() {
        logical
    } else if let Err(error) = std::env::set_current_dir(crate::bytes::to_path(target)) {
        let shown = if crate::escapes::needs_ansi_quote(target) {
            crate::escapes::ansi_quote(target)
        } else {
            target.to_string()
        };
        shell::error(format!("cd: {shown}: {}", sys::error_message(&error)));
        return 1;
    } else {
        std::env::current_dir().unwrap_or_else(|error| {
            shell::error(format!(
                "cd: error retrieving current directory: getcwd: cannot access parent directories: {}",
                sys::error_message(&error)
            ));
            crate::bytes::to_path(&previous).join(crate::bytes::to_path(target))
        })
    };
    let destination = crate::bytes::from_path(&destination);
    shell.export_variable("OLDPWD", Some(&previous));
    shell.export_variable("PWD", Some(&destination));
    if shell.interactive && destination != previous {
        shell.directories.visit(&destination, directories::now());
    }
    if destination != previous {
        shell.run_hook("chpwd", &[]);
    }
    0
}

pub(super) fn jump(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    match args.first().map(String::as_str) {
        None => cd(shell, &[], out),
        Some("-") => cd(shell, args, out),
        Some("-l") => {
            for (score, path) in shell.directories.ranked(&args[1..], directories::now()) {
                let _ = writeln!(out, "{score:>10.1}  {path}");
            }
            0
        }
        Some(single) if args.len() == 1 && crate::bytes::to_path(single).is_dir() => {
            change_directory(shell, single)
        }
        Some(_) => {
            let here = working_directory(shell);
            match shell
                .directories
                .best_match(args, directories::now(), &here)
            {
                Some(path) => change_directory(shell, &path),
                None => {
                    shell::error(format!("z: no match for '{}'", args.join(" ")));
                    1
                }
            }
        }
    }
}

fn working_directory(shell: &Shell) -> String {
    shell
        .get_variable("PWD")
        .filter(|path| path.starts_with('/'))
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|path| crate::bytes::from_path(&path))
        })
        .unwrap_or_else(|| "/".into())
}

pub(super) fn pwd(shell: &Shell, out: &mut dyn Write) -> i32 {
    let _ = writeln!(out, "{}", working_directory(shell));
    0
}

fn display_path(shell: &Shell, path: &str) -> String {
    match shell.get_variable("HOME") {
        Some(home)
            if !home.is_empty() && (path == home || path.starts_with(&format!("{home}/"))) =>
        {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

fn print_stack(shell: &Shell, out: &mut dyn Write) {
    let entries: Vec<String> = std::iter::once(working_directory(shell))
        .chain(shell.directory_stack.iter().rev().cloned())
        .map(|path| display_path(shell, &path))
        .collect();
    let _ = writeln!(out, "{}", entries.join(" "));
}

pub(super) fn pushd(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let current = working_directory(shell);
    let target = match args.first() {
        Some(target) => target.clone(),
        None => match shell.directory_stack.pop() {
            Some(top) => top,
            None => {
                shell::error("pushd: no other directory");
                return 1;
            }
        },
    };
    if change_directory(shell, &target) != 0 {
        if args.is_empty() {
            shell.directory_stack.push(target);
        }
        return 1;
    }
    shell.directory_stack.push(current);
    print_stack(shell, out);
    0
}

pub(super) fn popd(shell: &mut Shell, out: &mut dyn Write) -> i32 {
    let Some(target) = shell.directory_stack.pop() else {
        shell::error("popd: directory stack empty");
        return 1;
    };
    if change_directory(shell, &target) != 0 {
        return 1;
    }
    print_stack(shell, out);
    0
}

pub(super) fn dirs(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    match args.first().map(String::as_str) {
        None => print_stack(shell, out),
        Some("-c") => shell.directory_stack.clear(),
        Some("-v") => {
            let entries = std::iter::once(working_directory(shell))
                .chain(shell.directory_stack.iter().rev().cloned());
            for (index, path) in entries.enumerate() {
                let _ = writeln!(out, "{index:>2}  {}", display_path(shell, &path));
            }
        }
        Some(other) => {
            shell::error(format!("dirs: {other}: invalid option"));
            return 2;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_paths() {
        assert_eq!(logical_path("/a/b", ".."), PathBuf::from("/a"));
        assert_eq!(logical_path("/a/b", "../c/./d"), PathBuf::from("/a/c/d"));
        assert_eq!(logical_path("/a", "/x/../y"), PathBuf::from("/y"));
        assert_eq!(logical_path("/", ".."), PathBuf::from("/"));
    }
}
