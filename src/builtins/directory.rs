use std::io::Write;
use std::path::{Component, Path, PathBuf};

use crate::directories;
use crate::shell::{self, Shell};
use crate::sys;

fn logical_path(working_directory: &str, target: &str) -> PathBuf {
    let joined = if target.starts_with('/') {
        PathBuf::from(target)
    } else {
        Path::new(working_directory).join(target)
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
    change_directory(shell, &target)
}

pub(crate) fn change_directory(shell: &mut Shell, target: &str) -> i32 {
    let previous = working_directory(shell);
    let logical = logical_path(&previous, target);
    let destination = if std::env::set_current_dir(&logical).is_ok() {
        logical
    } else if let Err(error) = std::env::set_current_dir(target) {
        shell::error(format!("cd: {target}: {}", sys::error_message(&error)));
        return 1;
    } else {
        std::env::current_dir().unwrap_or(logical)
    };
    let destination = destination.to_string_lossy().into_owned();
    shell.export_variable("OLDPWD", Some(&previous));
    shell.export_variable("PWD", Some(&destination));
    if shell.interactive && destination != previous {
        shell.directories.visit(&destination, directories::now());
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
        Some(single) if args.len() == 1 && Path::new(single).is_dir() => {
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
                .map(|path| path.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "/".into())
}

pub(super) fn pwd(shell: &Shell, out: &mut dyn Write) -> i32 {
    let _ = writeln!(out, "{}", working_directory(shell));
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
