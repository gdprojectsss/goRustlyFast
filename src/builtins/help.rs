use std::io::Write;

use crate::shell;

const TOPICS: &[(&str, &str, &str)] = &[
    (
        ".",
        ". [-p path] file [args]",
        "run a file in the current shell, searching PATH for names without a slash",
    ),
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
    (
        "builtin",
        "builtin name [args]",
        "run a shell builtin, skipping functions with the same name",
    ),
    (
        "caller",
        "caller [frame]",
        "print the line and file of the current function call, or of an outer frame",
    ),
    (
        "cd",
        "cd [-L | -P [-e]] [dir | -]",
        "change the working directory, -P resolving symbolic links",
    ),
    (
        "command",
        "command [-pVv] name [args]",
        "run a command skipping functions, -p with the default PATH, -v/-V describe it",
    ),
    (
        "compgen",
        "compgen [-abcdefkuv] [-A action] [-W words] [-G glob] [-X filter] [-P prefix] [-S suffix] [word]",
        "print the completions that match word",
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
        "declare [-aAfFilprux] [name[=value] ...]",
        "set variable attributes or list variables",
    ),
    ("dirs", "dirs [-c | -v]", "show the directory stack"),
    (
        "disown",
        "disown [-h] [-ar] [%job | pid ...]",
        "remove jobs from the job table, or keep them on hangup with -h",
    ),
    ("echo", "echo [-neE] [args]", "print arguments"),
    (
        "enable",
        "enable [-a] [-nps] [name ...]",
        "list builtins, or turn them off with -n and back on",
    ),
    ("eval", "eval [args]", "run arguments as shell code"),
    (
        "exec",
        "exec [-cl] [-a name] [command [args]] [redirections]",
        "replace the shell or redirect its fds",
    ),
    ("exit", "exit [n]", "leave the shell"),
    (
        "export",
        "export [name[=value] ...]",
        "export variables to commands",
    ),
    (
        "explain",
        "explain command... | explain -f script",
        "show what a command line expands to without running it",
    ),
    ("false", "false", "fail"),
    (
        "fc",
        "fc [-e editor] [-lnr] [first [last]] | fc -s [old=new] [command]",
        "list, edit or run again commands from the history",
    ),
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
    (
        "jobs",
        "jobs [-lprs] [%job ...]",
        "list background and stopped jobs",
    ),
    (
        "kill",
        "kill [-s signal | -n number | -signal] pid | %job ... | -l [signal]",
        "send a signal, or list signal names",
    ),
    (
        "let",
        "let expression ...",
        "evaluate arithmetic, failing when the last value is 0",
    ),
    (
        "local",
        "local [-aA] name[=value] ...",
        "declare variables local to a function",
    ),
    (
        "mapfile",
        "mapfile [-d delim] [-n count] [-O origin] [-s count] [-t] [-u fd] [-C callback] [-c quantum] [array]",
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
    (
        "pwd",
        "pwd [-LP]",
        "print the working directory, -P with symbolic links resolved",
    ),
    (
        "read",
        "read [-ers] [-a array] [-d delim] [-n n] [-N n] [-p prompt] [-t secs] [-u fd] [names]",
        "read a line from input",
    ),
    (
        "readarray",
        "readarray [-d delim] [-n count] [-O origin] [-s count] [-t] [-u fd] [array]",
        "same as mapfile",
    ),
    (
        "readonly",
        "readonly [name[=value] ...]",
        "make variables read only",
    ),
    (
        "replace",
        "replace [--all] [-n] [-v] file old new | replace file < blocks | search ... | replace old new",
        "replace text that appears exactly once, writing the file atomically",
    ),
    ("return", "return [n]", "leave a function or sourced file"),
    (
        "set",
        "set [-abefhkmnptuvxBCEHPT] [-o option] [--] [args]",
        "set shell options or positional parameters; set -o lists them",
    ),
    ("shift", "shift [n]", "drop positional parameters"),
    (
        "shopt",
        "shopt [-pqsu] [-o] [name ...]",
        "set or show shell options such as globstar, nullglob, failglob, nocasematch",
    ),
    (
        "source",
        "source [-p path] file [args]",
        "run a file in the current shell, searching PATH for names without a slash",
    ),
    ("test", "test expression", "evaluate a condition"),
    (
        "trap",
        "trap [-lp] [action] [signal ...]",
        "run code on signals, EXIT, ERR, DEBUG or RETURN",
    ),
    (
        "timeout",
        "timeout [-s signal] [-k duration] [--preserve-status] duration command [args]",
        "run a command and stop its whole process group after a time limit",
    ),
    (
        "time",
        "time [-p] pipeline",
        "run a pipeline and report the elapsed, user and system time",
    ),
    (
        "times",
        "times",
        "show user and system time used by the shell and its children",
    ),
    ("true", "true", "succeed"),
    (
        "type",
        "type [-afptP] name ...",
        "show how a name would be run",
    ),
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
        "umask [-p] [-S] [mode]",
        "show or set the file creation mask",
    ),
    ("unalias", "unalias [-a] name ...", "remove aliases"),
    (
        "unset",
        "unset [-f] name ...",
        "remove variables, array elements or functions",
    ),
    (
        "wait",
        "wait [-fn] [-p var] [%job | pid ...]",
        "wait for background jobs and return their status",
    ),
    (
        "write-file",
        "write-file [-p] [-a] [-v] [-f] file < text",
        "write standard input to a file atomically (empty input needs -f)",
    ),
    (
        "z",
        "z [-l] term ...",
        "jump to a frequently used directory",
    ),
    (
        "files",
        "files [-a] [-r] [path...]",
        "table of files with name, type, size, modified",
    ),
    ("procs", "procs", "table of running processes"),
    (
        "git-status",
        "git-status [--ignored] [path...]",
        "table of changed files with state, staged and unstaged columns",
    ),
    (
        "ports",
        "ports",
        "table of listening TCP ports with the owning process",
    ),
    (
        "search",
        "search [-i] [-F] [-H] [-l] [-g glob] pattern [path...]",
        "search files by extended regex (\\d \\w \\s \\b work), skipping ignored, hidden and binary files",
    ),
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
        "keep matching rows: =~ !~ regex, has lacks substring, modified gt 2d is newer than 2 days",
    ),
    ("sort-by", "sort-by column... [-r]", "sort rows"),
    ("select", "select column...", "keep columns"),
    ("reject", "reject column...", "drop columns"),
    ("first", "first [n]", "keep the first n rows"),
    ("skip", "skip [n]", "drop the first n rows"),
    ("reverse", "reverse", "reverse the rows"),
    ("count", "count", "print the number of rows"),
    (
        "get",
        "get [-0] column",
        "print one column as plain lines, or NUL separated with -0",
    ),
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
            "\nhelp name shows usage. The docs live at https://grf.sh/docs/"
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
