use std::io::{self, Write};

use crate::shell::{self, Shell};

mod aliases;
mod control;
mod deparse;
mod directory;
mod files;
mod getopts;
mod help;
mod input;
mod interactive;
mod jobs;
mod lookup;
mod options;
mod output;
mod printf;
mod script;
mod system;
pub(crate) mod test;
mod timeout;
pub(crate) mod trap;
mod variables;

pub(crate) use directory::change_directory;
pub(crate) use lookup::{DEFAULT_PATH, is_active, path_matches, strip_prefixes};
pub(crate) use options::default_stored_shopts;
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
    "fc",
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
    "let",
    "builtin",
    "caller",
    "enable",
    "compgen",
];

pub(crate) fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

pub(crate) fn names() -> &'static [&'static str] {
    BUILTINS
}

pub(crate) fn run(shell: &mut Shell, argv: &[String]) -> i32 {
    let args = &argv[1..];
    let mut out = crate::bytes::Encoder::new(Output::new());
    let status = match argv[0].as_str() {
        "cd" => directory::cd(shell, args, &mut out),
        "z" => directory::jump(shell, args, &mut out),
        "pwd" => directory::pwd(shell, args, &mut out),
        "exit" => control::exit(shell, args),
        "export" | "local" | "declare" | "typeset" | "readonly" => {
            variables::declare(shell, &argv[0], args, &mut out)
        }
        "unset" => variables::unset(shell, args),
        "echo" => output::echo(args, &mut out),
        "alias" => aliases::alias(shell, args, &mut out),
        "unalias" => aliases::unalias(shell, args),
        "true" | ":" => 0,
        "false" => 1,
        "return" => control::return_from(shell, args),
        "break" | "continue" => control::loop_control(shell, &argv[0], args),
        "shift" => variables::shift(shell, args),
        "type" => lookup::type_command(shell, args, &mut out),
        "command" => lookup::command(shell, args, &mut out),
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
        "set" => options::set(shell, args, &mut out),
        "shopt" => options::shopt(shell, args, &mut out),
        "getopts" => getopts::run(shell, args),
        "mapfile" | "readarray" => input::mapfile(shell, args),
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
        "fc" => {
            drop(out);
            return crate::repl::fc(shell, args);
        }
        "read" => input::read(shell, args),
        "source" | "." => {
            drop(out);
            return script::source(shell, &argv[0], args);
        }
        "eval" => {
            drop(out);
            return shell.run_source(&args.join(" "));
        }
        "let" => control::let_command(shell, args),
        "builtin" => {
            drop(out);
            return lookup::builtin(shell, args);
        }
        "caller" => control::caller(shell, args, &mut out),
        "enable" => lookup::enable(shell, args, &mut out),
        "compgen" => lookup::compgen(shell, args, &mut out),
        _ => unreachable!(),
    };
    let _ = out.flush();
    match out.inner_mut().error.take() {
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

pub(super) fn parse_count(
    shell: &mut Shell,
    command: &str,
    args: &[String],
    default: i64,
) -> Result<i64, i32> {
    let args = match args.first() {
        Some(first) if first == "--" => &args[1..],
        _ => args,
    };
    let Some(argument) = args.first() else {
        return Ok(default);
    };
    let Ok(value) = argument.trim().parse::<i64>() else {
        shell::error(format!("{command}: {argument}: numeric argument required"));
        return Err(2);
    };
    if args.len() > 1 {
        shell::error(format!("{command}: too many arguments"));
        if shell.command_string && !shell.interactive {
            shell.flow = shell::Flow::Exit;
            return Err(1);
        }
        shell.flow = shell::Flow::Abort;
        return Err(2);
    }
    Ok(value)
}
