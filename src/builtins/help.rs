use std::io::Write;

use crate::shell;

const TOPICS: &[(&str, &str, &str)] = &[
    (".", ". file [args]", "run a file in the current shell"),
    (":", ": [args]", "do nothing and succeed"),
    ("[", "[ expression ]", "evaluate a condition, same as test"),
    (
        "abbr",
        "abbr [name expansion] | -e name",
        "define abbreviations expanded on Enter",
    ),
    ("alias", "alias [name=value ...]", "define or list aliases"),
    (
        "bg",
        "bg [%job]",
        "continue a stopped job in the background",
    ),
    (
        "bindkey",
        "bindkey [key action] | -s key text | -l",
        "bind keys in the line editor",
    ),
    ("break", "break [n]", "leave the innermost n loops"),
    ("cd", "cd [dir | -]", "change the working directory"),
    (
        "command",
        "command [-v] name [args]",
        "run a command, skipping functions",
    ),
    (
        "complete",
        "complete [-W words] [-C cmd] [-d] [-f] name",
        "add tab completion for a command",
    ),
    (
        "configure-prompt",
        "configure-prompt [--preset name | --list]",
        "set up the prompt theme",
    ),
    (
        "continue",
        "continue [n]",
        "start the next iteration of a loop",
    ),
    (
        "declare",
        "declare [-aAilrux] [name[=value] ...]",
        "set variable attributes or list variables",
    ),
    ("dirs", "dirs [-c | -v]", "show the directory stack"),
    ("echo", "echo [-neE] [args]", "print arguments"),
    ("eval", "eval [args]", "run arguments as shell code"),
    (
        "exec",
        "exec [command] [redirections]",
        "replace the shell or redirect its fds",
    ),
    ("exit", "exit [n]", "leave the shell"),
    (
        "export",
        "export [name[=value] ...]",
        "export variables to commands",
    ),
    ("false", "false", "fail"),
    ("fg", "fg [%job]", "bring a job to the foreground"),
    (
        "getopts",
        "getopts spec name [args]",
        "parse command line options",
    ),
    (
        "help",
        "help [name]",
        "show this list or details for one command",
    ),
    (
        "history",
        "history [n | -c | --table]",
        "show history, or the command log as a table",
    ),
    ("jobs", "jobs", "list background and stopped jobs"),
    ("kill", "kill [-signal] pid | %job", "send a signal"),
    (
        "local",
        "local [-aA] name[=value] ...",
        "declare variables local to a function",
    ),
    (
        "mapfile",
        "mapfile [-t] [-n count] [-s skip] [array]",
        "read lines into an array",
    ),
    ("popd", "popd", "leave the top directory of the stack"),
    (
        "printf",
        "printf [-v var] format [args]",
        "print formatted text",
    ),
    (
        "pushd",
        "pushd [dir]",
        "push a directory onto the stack and enter it",
    ),
    ("pwd", "pwd", "print the working directory"),
    (
        "read",
        "read [-r] [-a array] [-n n] [-t secs] [-s] [-d delim] [-p prompt] [names]",
        "read a line from input",
    ),
    ("readarray", "readarray [-t] [array]", "same as mapfile"),
    (
        "readonly",
        "readonly [name[=value] ...]",
        "make variables read only",
    ),
    ("return", "return [n]", "leave a function or sourced file"),
    (
        "set",
        "set [-euxo option] [--] [args]",
        "set shell options or positional parameters",
    ),
    ("shift", "shift [n]", "drop positional parameters"),
    (
        "shopt",
        "shopt [-s | -u | -q] [name ...]",
        "glob options: globstar nullglob dotglob nocaseglob",
    ),
    (
        "source",
        "source file [args]",
        "run a file in the current shell",
    ),
    ("test", "test expression", "evaluate a condition"),
    (
        "trap",
        "trap [action] [signal ...]",
        "run code on signals, EXIT or ERR",
    ),
    (
        "timeout",
        "timeout [-s signal] [-k duration] duration command [args]",
        "run a command and stop its whole process group after a time limit",
    ),
    ("true", "true", "succeed"),
    ("type", "type name ...", "show how a name would be run"),
    (
        "typeset",
        "typeset [-aAilrux] [name[=value] ...]",
        "same as declare",
    ),
    (
        "ulimit",
        "ulimit [-SHa] [-cdfnstuv] [limit]",
        "show or set resource limits",
    ),
    (
        "umask",
        "umask [-S] [mode]",
        "show or set the file creation mask",
    ),
    ("unalias", "unalias [-a] name ...", "remove aliases"),
    (
        "unset",
        "unset [-f] name ...",
        "remove variables, array elements or functions",
    ),
    ("wait", "wait [-n] [%job]", "wait for background jobs"),
    (
        "z",
        "z [-l] term ...",
        "jump to a frequently used directory",
    ),
    (
        "files",
        "files [path] [-a] [-r]",
        "table of files with name, type, size, modified",
    ),
    ("procs", "procs", "table of running processes"),
    (
        "load",
        "load file",
        "read csv, tsv, json or lines into a table",
    ),
    (
        "from",
        "from csv|tsv|json|jsonl|ws|lines",
        "parse piped text into a table",
    ),
    (
        "to",
        "to csv|tsv|json|md|html|table",
        "turn a table into text",
    ),
    (
        "where",
        "where column op value [and|or ...]",
        "keep matching rows",
    ),
    ("sort-by", "sort-by column... [-r]", "sort rows"),
    ("select", "select column...", "keep columns"),
    ("reject", "reject column...", "drop columns"),
    ("first", "first [n]", "keep the first n rows"),
    ("skip", "skip [n]", "drop the first n rows"),
    ("reverse", "reverse", "reverse the rows"),
    ("count", "count", "print the number of rows"),
    ("get", "get column", "print one column as plain lines"),
    ("group-by", "group-by column", "count rows per value"),
    ("uniq-by", "uniq-by column", "keep the first row per value"),
    (
        "math",
        "math sum|avg|min|max|median column",
        "aggregate a column",
    ),
    ("update", "update column value", "set or add a column"),
];

pub(super) fn run(args: &[String], out: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = writeln!(
            out,
            "grfsh {}. Builtins and table commands:\n",
            env!("CARGO_PKG_VERSION")
        );
        for (name, _, description) in TOPICS {
            let _ = writeln!(out, "  {name:<18}{description}");
        }
        let _ = writeln!(
            out,
            "\nhelp name shows usage. The docs live at https://github.com/gdprojectsss/goRustlyFast"
        );
        return 0;
    }
    let mut status = 0;
    for name in args {
        match TOPICS.iter().find(|(topic, _, _)| topic == name) {
            Some((_, usage, description)) => {
                let _ = writeln!(out, "{usage}\n    {description}");
            }
            None => {
                shell::error(format!("help: no help for '{name}'"));
                status = 1;
            }
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_and_table_command_has_help() {
        let topics: Vec<&str> = TOPICS.iter().map(|(name, _, _)| *name).collect();
        for name in super::super::names().iter().chain(crate::table::names()) {
            assert!(topics.contains(name), "missing help for {name}");
        }
    }
}
