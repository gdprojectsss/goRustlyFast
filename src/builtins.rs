use std::collections::HashMap;
use std::io::Write;

use crate::parser::Command;
use crate::shell;

pub fn is_builtin(name: &str) -> bool {
    matches!(name, "cd" | "exit" | "export" | "echo" | "alias" | "unalias")
}

pub fn run(cmd: &Command, out: &mut dyn Write, aliases: &mut HashMap<String, String>) -> i32 {
    match cmd.program.as_str() {
        "cd" => builtin_cd(&cmd.args, out),
        "exit" => builtin_exit(&cmd.args),
        "export" => builtin_export(&cmd.args),
        "echo" => builtin_echo(&cmd.args, out),
        "alias" => builtin_alias(&cmd.args, out, aliases),
        "unalias" => builtin_unalias(&cmd.args, aliases),
        _ => unreachable!(),
    }
}

fn builtin_cd(args: &[String], out: &mut dyn Write) -> i32 {
    let target = match args.first().map(String::as_str) {
        None | Some("~") => match std::env::var("HOME") {
            Ok(home) => home,
            Err(_) => {
                shell::error("cd: HOME not set");
                return 1;
            }
        },
        Some("-") => match std::env::var("OLDPWD") {
            Ok(prev) => {
                let _ = writeln!(out, "{prev}");
                prev
            }
            Err(_) => {
                shell::error("cd: OLDPWD not set");
                return 1;
            }
        },
        Some(dir) => dir.to_string(),
    };

    let current = std::env::current_dir().ok();

    if let Err(e) = std::env::set_current_dir(&target) {
        shell::error(format!("cd: {target}: {e}"));
        return 1;
    }

    if let Some(prev) = current {
        unsafe { std::env::set_var("OLDPWD", prev) };
    }

    0
}

fn builtin_exit(args: &[String]) -> ! {
    let code = args
        .first()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    std::process::exit(code);
}

fn builtin_export(args: &[String]) -> i32 {
    for arg in args {
        if let Some((key, value)) = arg.split_once('=') {
            unsafe { std::env::set_var(key, value) };
        }
    }
    0
}

fn builtin_echo(args: &[String], out: &mut dyn Write) -> i32 {
    if args.first().is_some_and(|a| a == "-n") {
        let _ = write!(out, "{}", args[1..].join(" "));
    } else {
        let _ = writeln!(out, "{}", args.join(" "));
    }
    0
}

fn builtin_alias(
    args: &[String],
    out: &mut dyn Write,
    aliases: &mut HashMap<String, String>,
) -> i32 {
    if args.is_empty() {
        let mut sorted: Vec<_> = aliases.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        for (name, value) in sorted {
            let _ = writeln!(out, "alias {name}='{value}'");
        }
        return 0;
    }

    for arg in args {
        match arg.split_once('=') {
            Some((name, value)) => {
                aliases.insert(name.into(), value.into());
            }
            None => match aliases.get(arg.as_str()) {
                Some(value) => {
                    let _ = writeln!(out, "alias {arg}='{value}'");
                }
                None => {
                    shell::error(format!("alias: {arg}: not found"));
                    return 1;
                }
            },
        }
    }
    0
}

fn builtin_unalias(args: &[String], aliases: &mut HashMap<String, String>) -> i32 {
    for arg in args {
        aliases.remove(arg.as_str());
    }
    0
}
