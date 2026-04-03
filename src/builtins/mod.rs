use std::io::{self, Write};

use crate::shell::{self, Shell};

mod control;
mod directory;
mod jobs;
mod output;
mod variables;

const BUILTINS: &[&str] = &[
    "cd", "pwd", "exit", "export", "unset", "echo", "alias", "unalias", "true", "false", ":",
    "return", "break", "continue", "local", "shift", "type", "command", "eval", "jobs", "fg", "bg",
    "wait", "kill",
];

pub(crate) fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

pub(crate) fn names() -> &'static [&'static str] {
    BUILTINS
}

pub(crate) fn run(shell: &mut Shell, argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut out = io::stdout().lock();
    let status = match argv[0].as_str() {
        "cd" => directory::cd(shell, args, &mut out),
        "pwd" => directory::pwd(shell, &mut out),
        "exit" => control::exit(shell, args),
        "export" => variables::export(shell, args, &mut out),
        "unset" => variables::unset(shell, args),
        "echo" => output::echo(args, &mut out),
        "alias" => variables::alias(shell, args, &mut out),
        "unalias" => variables::unalias(shell, args),
        "true" | ":" => 0,
        "false" => 1,
        "return" => control::return_from(shell, args),
        "break" | "continue" => control::loop_control(shell, &argv[0], args),
        "local" => variables::local(shell, args),
        "shift" => variables::shift(shell, args),
        "type" => output::describe(shell, args, &mut out, false),
        "command" => output::describe(shell, &args[args.len().min(1)..], &mut out, true),
        "jobs" => jobs::list(shell, &mut out),
        "fg" | "bg" | "wait" => {
            drop(out);
            return match argv[0].as_str() {
                "fg" => jobs::fg(shell, args),
                "bg" => jobs::bg(shell, args),
                _ => jobs::wait(shell, args),
            };
        }
        "kill" => jobs::kill(shell, args, &mut out),
        "eval" => {
            drop(out);
            return shell.run_source(&args.join(" "));
        }
        _ => unreachable!(),
    };
    let _ = out.flush();
    status
}

pub(super) fn parse_count(command: &str, args: &[String], default: i32) -> Result<i32, i32> {
    match args.first() {
        None => Ok(default),
        Some(argument) => argument.parse().map_err(|_| {
            shell::error(format!("{command}: {argument}: numeric argument required"));
            2
        }),
    }
}
