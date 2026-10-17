use std::io::{self, Write};

use crate::shell::{self, Shell};

mod control;
mod directory;
mod files;
mod getopts;
mod help;
mod interactive;
mod jobs;
mod output;
mod printf;
mod script;
mod system;
pub(crate) mod test;
mod timeout;
pub(crate) mod trap;
mod variables;

pub(crate) use directory::change_directory;
pub(crate) use variables::attribute_letters;

const BUILTINS: &[&str] = &[
    "cd",
    "z",
    "pwd",
    "exit",
    "export",
    "unset",
    "echo",
    "alias",
    "unalias",
    "true",
    "false",
    ":",
    "return",
    "break",
    "continue",
    "local",
    "shift",
    "type",
    "command",
    "eval",
    "jobs",
    "fg",
    "bg",
    "wait",
    "kill",
    "disown",
    "times",
    "source",
    ".",
    "read",
    "set",
    "test",
    "[",
    "trap",
    "printf",
    "pushd",
    "popd",
    "dirs",
    "history",
    "declare",
    "typeset",
    "readonly",
    "getopts",
    "shopt",
    "mapfile",
    "readarray",
    "exec",
    "umask",
    "ulimit",
    "configure-prompt",
    "abbr",
    "bindkey",
    "complete",
    "help",
    "timeout",
    "replace",
    "write-file",
    "explain",
];

pub(crate) fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

pub(crate) fn names() -> &'static [&'static str] {
    BUILTINS
}

pub(crate) fn run(shell: &mut Shell, argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut out = Output::new();
    let status = match argv[0].as_str() {
        "cd" => directory::cd(shell, args, &mut out),
        "z" => directory::jump(shell, args, &mut out),
        "pwd" => directory::pwd(shell, &mut out),
        "exit" => control::exit(shell, args),
        "export" | "local" | "declare" | "typeset" | "readonly" => {
            variables::declare(shell, &argv[0], args, &mut out)
        }
        "unset" => variables::unset(shell, args),
        "echo" => output::echo(args, &mut out),
        "alias" => variables::alias(shell, args, &mut out),
        "unalias" => variables::unalias(shell, args),
        "true" | ":" => 0,
        "false" => 1,
        "return" => control::return_from(shell, args),
        "break" | "continue" => control::loop_control(shell, &argv[0], args),
        "shift" => variables::shift(shell, args),
        "type" => output::describe(shell, args, &mut out, false),
        "command" => output::describe(shell, &args[args.len().min(1)..], &mut out, true),
        "jobs" => jobs::list(shell, args, &mut out),
        "fg" => jobs::fg(shell, args, &mut out),
        "bg" => jobs::bg(shell, args, &mut out),
        "wait" => {
            drop(out);
            return jobs::wait(shell, args);
        }
        "kill" => jobs::kill(shell, args, &mut out),
        "disown" => jobs::disown(shell, args),
        "times" => jobs::times(&mut out),
        "set" => script::set(shell, args, &mut out),
        "shopt" => script::shopt(shell, args, &mut out),
        "getopts" => getopts::run(shell, args),
        "mapfile" | "readarray" => script::mapfile(shell, args),
        "umask" => system::umask(args, &mut out),
        "abbr" => interactive::abbr(shell, args, &mut out),
        "help" => help::run(args, &mut out),
        "bindkey" => interactive::bindkey(shell, args, &mut out),
        "complete" => interactive::complete(shell, args, &mut out),
        "configure-prompt" => {
            drop(out);
            let directory = crate::repl::config_dir();
            return crate::repl::theme::wizard::run(shell, &directory, args);
        }
        "ulimit" => system::ulimit(args, &mut out),
        "replace" => files::replace(args, &mut out),
        "write-file" => files::write_file(args, &mut out),
        "explain" => {
            drop(out);
            return explain(shell, args);
        }
        "timeout" => {
            drop(out);
            return timeout::run(shell, args);
        }
        "test" | "[" => test::run(shell, &argv[0], args),
        "trap" => trap::run(shell, args, &mut out),
        "printf" => printf::run(shell, args, &mut out),
        "pushd" => directory::pushd(shell, args, &mut out),
        "popd" => directory::popd(shell, &mut out),
        "dirs" => directory::dirs(shell, args, &mut out),
        "history" => output::history(shell, args, &mut out),
        "read" => script::read(shell, args),
        "source" | "." => {
            drop(out);
            return script::source(shell, args);
        }
        "eval" => {
            drop(out);
            return shell.run_source(&args.join(" "));
        }
        _ => unreachable!(),
    };
    let _ = out.flush();
    match out.error.take() {
        Some(error) => report_write_error(shell, &argv[0], &error),
        None => status,
    }
}

fn report_write_error(shell: &Shell, name: &str, error: &io::Error) -> i32 {
    let broken_pipe = error.raw_os_error() == Some(libc::EPIPE);
    let pipe_signal_handled = broken_pipe
        && crate::signals::is_pending(libc::SIGPIPE)
        && !shell.traps.contains_key("PIPE");
    if !pipe_signal_handled {
        shell::error(format!(
            "{name}: write error: {}",
            crate::sys::error_message(error)
        ));
    }
    1
}

struct Output {
    buffer: Vec<u8>,
    error: Option<io::Error>,
}

impl Output {
    fn new() -> Self {
        let _ = io::stdout().flush();
        Output {
            buffer: Vec::new(),
            error: None,
        }
    }

    fn send(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let result = crate::sys::write_all(1, &self.buffer);
        self.buffer.clear();
        if let Err(error) = &result
            && self.error.is_none()
        {
            self.error = Some(io::Error::from_raw_os_error(
                error.raw_os_error().unwrap_or(libc::EIO),
            ));
        }
        result
    }
}

impl Write for Output {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        self.buffer.extend_from_slice(data);
        if self.buffer.len() >= 8192 {
            self.send()?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.error.is_some() {
            self.buffer.clear();
            return Ok(());
        }
        self.send()
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn explain(shell: &mut Shell, args: &[String]) -> i32 {
    match crate::sys::fork() {
        Ok(0) => {
            shell.enter_child(None, false);
            let status = crate::agent::explain_arguments(shell, args);
            crate::sys::exit_now(status)
        }
        Ok(pid) => crate::sys::waitpid(pid, 0)
            .ok()
            .flatten()
            .map_or(1, crate::sys::WaitResult::status),
        Err(error) => {
            shell::error(format!("fork: {}", crate::sys::error_message(&error)));
            1
        }
    }
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
