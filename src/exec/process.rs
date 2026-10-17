use crate::jobs::JobState;
use crate::shell::{self, Flow, Shell};
use crate::signals;
use crate::syntax::ast::{AndOr, Command, Redirect};
use crate::syntax::parser;
use crate::sys::{self, WaitResult};
use crate::table;

impl Shell {
    pub(crate) fn run_background(&mut self, item: &AndOr) {
        let log = self.agent.then(crate::agent::job_log_path).flatten();
        match sys::fork() {
            Ok(0) => {
                self.enter_child(Some(0), false);
                if let Some(path) = &log {
                    crate::agent::send_output_to(path);
                }
                let status =
                    if item.rest.is_empty() && !item.first.negate && item.first.commands.len() == 1
                    {
                        self.run_command(&item.first.commands[0], true)
                    } else {
                        self.run_and_or(item)
                    };
                sys::exit_now(status)
            }
            Ok(pid) => {
                sys::setpgid(pid, pid);
                self.started_background(pid, &item.text);
                if let Some(path) = &log {
                    eprintln!(
                        "grfsh: background job {pid} writes its output to {}",
                        path.display()
                    );
                }
                self.last_background_pid = Some(pid);
                self.last_status = 0;
            }
            Err(error) => shell::error(format!("fork: {}", sys::error_message(&error))),
        }
    }

    pub(crate) fn enter_child(&mut self, pgid: Option<i32>, foreground: bool) {
        if self.agent {
            crate::agent::detach_from_terminal();
        } else if let Some(pgid) = pgid {
            let pgid = if pgid == 0 { sys::getpid() } else { pgid };
            sys::setpgid(0, pgid);
            if foreground && self.interactive {
                sys::tcsetpgrp(pgid);
            }
        }
        sys::reset_child_signals();
        self.traps.retain(|name, command| {
            let ignored = command.is_empty();
            if !ignored && let Some(signal) = sys::signal_number(name) {
                signals::set(signal, signals::Disposition::Default);
            }
            ignored
        });
        if self.interactive {
            signals::set(libc::SIGTERM, signals::Disposition::Default);
            signals::set(libc::SIGHUP, signals::Disposition::Default);
        }
        signals::clear();
        self.interactive = false;
        self.forget_jobs();
    }

    pub(crate) fn run_forked(&mut self, commands: &[Command], text: &str) -> i32 {
        let stage_count = commands.len();
        let mut pipes = Vec::with_capacity(stage_count.saturating_sub(1));
        for _ in 1..stage_count {
            match sys::pipe() {
                Ok(pipe) => pipes.push(pipe),
                Err(error) => {
                    close_pipes(&pipes);
                    shell::error(format!("pipe: {}", sys::error_message(&error)));
                    return 1;
                }
            }
        }

        let mut pids = Vec::with_capacity(stage_count);
        let mut pgid = 0;
        for (index, command) in commands.iter().enumerate() {
            match sys::fork() {
                Ok(0) => {
                    self.enter_child(self.interactive.then_some(pgid), true);
                    if index > 0 {
                        let _ = sys::dup2(pipes[index - 1].0, 0);
                    }
                    if index + 1 < stage_count {
                        let _ = sys::dup2(pipes[index].1, 1);
                    }
                    close_pipes(&pipes);
                    self.pipe_to_table =
                        commands.get(index + 1).is_some_and(table::consumes_tables);
                    let status = self.run_command(command, true);
                    sys::exit_now(status)
                }
                Ok(pid) => {
                    if pgid == 0 {
                        pgid = pid;
                    }
                    if self.interactive {
                        sys::setpgid(pid, pgid);
                    }
                    pids.push(pid);
                }
                Err(error) => {
                    shell::error(format!("fork: {}", sys::error_message(&error)));
                    break;
                }
            }
        }
        close_pipes(&pipes);

        if pids.is_empty() {
            return 1;
        }
        self.wait_foreground(pgid, pids, text)
    }

    pub(crate) fn exec_external(
        &mut self,
        path: &str,
        argv: &[String],
        assignments: &[(String, String)],
        redirects: &[Redirect],
    ) -> ! {
        if let Err(error) = self.apply_redirects(redirects) {
            shell::error(error);
            sys::exit_now(1);
        }
        for (name, value) in assignments {
            unsafe { std::env::set_var(name, value) };
        }
        let err = sys::execv(path, argv);
        shell::error(format!("{}: {}", argv[0], sys::error_message(&err)));
        sys::exit_now(126)
    }

    pub(crate) fn process_substitution(
        &mut self,
        source: &str,
        output: bool,
    ) -> Result<String, String> {
        if self.explaining {
            return Ok(format!("{}({source})", if output { '>' } else { '<' }));
        }
        let (read_end, write_end) =
            sys::pipe().map_err(|error| format!("pipe: {}", sys::error_message(&error)))?;
        let (kept, given) = if output {
            (write_end, read_end)
        } else {
            (read_end, write_end)
        };
        match sys::fork() {
            Ok(0) => {
                self.enter_child(None, false);
                sys::close(kept);
                let _ = sys::dup2(given, if output { 0 } else { 1 });
                sys::close(given);
                let status = match parser::parse_from_line(
                    source,
                    &self.aliases,
                    self.current_line.max(1),
                ) {
                    Ok(list) => self.run_list_exec(&list),
                    Err(error) => {
                        shell::error(error);
                        2
                    }
                };
                sys::exit_now(status)
            }
            Ok(pid) => {
                sys::close(given);
                sys::clear_cloexec(kept);
                self.substitution_fds.push(kept);
                self.substitution_pids.push(pid);
                Ok(format!("/dev/fd/{kept}"))
            }
            Err(error) => {
                sys::close(read_end);
                sys::close(write_end);
                Err(format!("fork: {}", sys::error_message(&error)))
            }
        }
    }

    pub(crate) fn capture(&mut self, source: &str) -> String {
        if self.explaining {
            return format!("$({})", source.trim());
        }
        if let Some(path) = file_read_shortcut(source) {
            let path = match crate::syntax::lexer::parse_double_quoted_text(&path)
                .map_err(|error| error.to_string())
                .and_then(|word| crate::expand::single(self, &word))
            {
                Ok(path) => path,
                Err(error) => {
                    shell::error(error);
                    return String::new();
                }
            };
            return match std::fs::read_to_string(&path) {
                Ok(mut text) => {
                    while text.ends_with('\n') {
                        text.pop();
                    }
                    self.substitution_status = Some(0);
                    text
                }
                Err(error) => {
                    shell::error(format!("{path}: {}", sys::error_message(&error)));
                    self.substitution_status = Some(1);
                    String::new()
                }
            };
        }
        let (read_end, write_end) = match sys::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                shell::error(format!("pipe: {}", sys::error_message(&error)));
                return String::new();
            }
        };
        match sys::fork() {
            Ok(0) => {
                self.enter_child(None, false);
                sys::close(read_end);
                let _ = sys::dup2(write_end, 1);
                sys::close(write_end);
                let first_line = self.current_line.max(1);
                let status = match parser::parse_from_line(source, &self.aliases, first_line) {
                    Ok(list) => self.run_list_exec(&list),
                    Err(error) => {
                        shell::error(error);
                        2
                    }
                };
                sys::exit_now(status)
            }
            Ok(pid) => {
                sys::close(write_end);
                let out = sys::read_all(read_end);
                sys::close(read_end);
                let status = sys::waitpid(pid, 0)
                    .ok()
                    .flatten()
                    .map_or(1, WaitResult::status);
                self.substitution_status = Some(status);
                let mut output = String::from_utf8_lossy(&out).into_owned();
                while output.ends_with('\n') {
                    output.pop();
                }
                output
            }
            Err(error) => {
                sys::close(read_end);
                sys::close(write_end);
                shell::error(format!("fork: {}", sys::error_message(&error)));
                String::new()
            }
        }
    }

    pub(crate) fn wait_foreground(&mut self, pgid: i32, pids: Vec<i32>, text: &str) -> i32 {
        self.wait_job(None, pgid, pids, text)
    }

    pub(crate) fn wait_job(
        &mut self,
        id: Option<usize>,
        pgid: i32,
        pids: Vec<i32>,
        text: &str,
    ) -> i32 {
        if self.interactive {
            sys::tcsetpgrp(pgid);
        }
        if self.agent {
            crate::agent::register(&pids);
        }
        let mut statuses = Vec::with_capacity(pids.len());
        let mut stopped = Vec::new();
        let mut interrupted = false;
        for &pid in &pids {
            match sys::waitpid(pid, libc::WUNTRACED) {
                Ok(Some(WaitResult::Stopped)) => stopped.push(pid),
                Ok(Some(result)) => {
                    statuses.push(result.status());
                    interrupted |= result == WaitResult::Signaled(libc::SIGINT);
                }
                _ => statuses.push(0),
            }
        }
        if self.agent {
            crate::agent::unregister(&pids);
        }
        self.pipe_statuses = statuses.clone();
        let status = if self.options.pipefail {
            statuses
                .iter()
                .rev()
                .copied()
                .find(|status| *status != 0)
                .unwrap_or(0)
        } else {
            statuses.last().copied().unwrap_or(0)
        };
        if self.interactive {
            sys::tcsetpgrp(self.shell_pgid);
        }

        if !stopped.is_empty() {
            let id = self.jobs.add(id, pgid, stopped, text, JobState::Stopped);
            eprintln!("\n[{id}]+  Stopped    {text}");
            if self.interactive {
                self.flow = Flow::Interrupted;
            }
            return 128 + libc::SIGTSTP;
        }
        if interrupted && self.interactive {
            eprintln!();
            self.flow = Flow::Interrupted;
        }
        status
    }

    pub(crate) fn started_background(&mut self, pid: i32, text: &str) {
        let id = self.jobs.add(None, pid, vec![pid], text, JobState::Running);
        if self.interactive {
            eprintln!("[{id}] {pid}");
        }
    }

    pub(crate) fn forget_jobs(&mut self) {
        self.jobs.clear();
    }

    pub(crate) fn reap_jobs(&mut self) {
        self.substitution_pids
            .retain(|&pid| matches!(sys::waitpid(pid, libc::WNOHANG), Ok(None)));
        for notice in self.jobs.reap() {
            if self.interactive {
                eprintln!("{notice}");
            }
        }
    }
}

fn close_pipes(pipes: &[(i32, i32)]) {
    for &(read_end, write_end) in pipes {
        sys::close(read_end);
        sys::close(write_end);
    }
}

fn file_read_shortcut(source: &str) -> Option<String> {
    let trimmed = source.trim();
    let rest = trimmed.strip_prefix('<')?.trim_start();
    if rest.is_empty() || rest.starts_with(['<', '(', '>']) || rest.contains(char::is_whitespace) {
        return None;
    }
    let path = rest.trim_matches('"').trim_matches('\'');
    Some(path.to_string())
}
