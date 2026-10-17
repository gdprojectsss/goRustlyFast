use crate::builtins::trap;
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
                let job_control = self.interactive;
                self.enter_child(Some(0), false);
                if !job_control && !self.agent {
                    signals::keep_ignored(libc::SIGINT);
                    signals::keep_ignored(libc::SIGQUIT);
                }
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
                self.exit_child(status)
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
        let errtrace = self.options.errtrace;
        let functrace = self.options.functrace;
        self.traps.retain(|name, command| {
            let kept = command.is_empty()
                || (errtrace && name == "ERR")
                || (functrace && (name == "DEBUG" || name == "RETURN"));
            if let Some(signal) = sys::signal_number(name) {
                let disposition = if kept {
                    signals::Disposition::Ignore
                } else {
                    signals::Disposition::Default
                };
                signals::set(signal, disposition);
            }
            kept
        });
        if self.interactive {
            signals::set(libc::SIGTERM, signals::Disposition::Default);
            signals::set(libc::SIGHUP, signals::Disposition::Default);
        }
        signals::clear();
        self.interactive = false;
        trap::refresh_terminating(self);
        self.jobs.inherit();
    }

    pub(crate) fn exit_child(&mut self, status: i32) -> ! {
        self.last_status = status;
        self.run_exit_trap();
        sys::exit_now(self.last_status)
    }

    pub(crate) fn die_by_signal(&mut self, signal: libc::c_int) -> ! {
        signals::take(signal);
        self.last_status = 128 + signal;
        self.run_exit_trap();
        sys::die_by_signal(signal)
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
                    if text == super::SUBSHELL_TEXT {
                        self.jobs.clear();
                    }
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
                    self.exit_child(status)
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
        let error = sys::execv(path, argv);
        if error.raw_os_error() == Some(libc::ENOEXEC) {
            self.run_as_script(path, argv);
        }
        let message = if std::path::Path::new(path).is_dir() {
            "Is a directory".to_string()
        } else {
            sys::error_message(&error)
        };
        shell::error(format!("{}: {message}", argv[0]));
        let status = if error.kind() == std::io::ErrorKind::NotFound {
            127
        } else {
            126
        };
        sys::exit_now(status)
    }

    fn run_as_script(&mut self, path: &str, argv: &[String]) {
        if is_binary_file(path) {
            shell::error(format!(
                "{}: cannot execute binary file: Exec format error",
                argv[0]
            ));
            sys::exit_now(126);
        }
        let Ok(program) = std::env::current_exe() else {
            return;
        };
        let program = program.to_string_lossy().into_owned();
        let script = if argv[0].contains('/') {
            argv[0].clone()
        } else {
            path.to_string()
        };
        let arguments: Vec<String> = ["grfsh".to_string(), script]
            .into_iter()
            .chain(argv[1..].iter().cloned())
            .collect();
        let _ = sys::execv(&program, &arguments);
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
                    self.active_aliases(),
                    self.current_line.max(1),
                ) {
                    Ok(list) => self.run_list_exec(&list),
                    Err(error) => {
                        shell::error(error);
                        2
                    }
                };
                self.exit_child(status)
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
            return match crate::sandbox::check_read(&path)
                .and_then(|()| crate::bytes::read_file(&path))
            {
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
                if !self.options.inherit_errexit {
                    self.options.errexit = false;
                }
                sys::close(read_end);
                let _ = sys::dup2(write_end, 1);
                sys::close(write_end);
                let first_line = self.current_line.max(1);
                let status =
                    match parser::parse_from_line(source, self.active_aliases(), first_line) {
                        Ok(list) => self.run_list_exec(&list),
                        Err(error) => {
                            shell::error(error);
                            2
                        }
                    };
                self.exit_child(status)
            }
            Ok(pid) => {
                sys::close(write_end);
                let out = sys::read_to_end(read_end);
                sys::close(read_end);
                let status = sys::waitpid(pid, 0)
                    .ok()
                    .flatten()
                    .map_or(1, WaitResult::status);
                self.substitution_status = Some(status);
                let mut output = crate::bytes::decode(&out);
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
        let cooperative = !self.interactive && !self.traps.contains_key("INT");
        if cooperative {
            signals::set(libc::SIGINT, signals::Disposition::Catch);
        }
        for &pid in &pids {
            match self.wait_foreground_pid(pid, cooperative) {
                Ok(Some(WaitResult::Stopped)) => stopped.push(pid),
                Ok(Some(result)) => {
                    statuses.push(result.status());
                    interrupted |= result == WaitResult::Signaled(libc::SIGINT);
                }
                _ => statuses.push(0),
            }
        }
        if cooperative {
            signals::set(libc::SIGINT, trap::default_disposition(self, libc::SIGINT));
            if signals::take(libc::SIGINT) && interrupted {
                self.die_by_signal(libc::SIGINT);
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
            if let Some(index) = self.jobs.index_of(id) {
                eprintln!("\n{}", self.jobs.get(index).line('+', false));
            }
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

    fn wait_foreground_pid(
        &mut self,
        pid: i32,
        cooperative: bool,
    ) -> std::io::Result<Option<WaitResult>> {
        loop {
            match sys::waitpid_interruptible(pid, libc::WUNTRACED) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                    if let Some(signal) = self.fatal_pending_signal(cooperative) {
                        self.die_by_signal(signal);
                    }
                }
                other => return other,
            }
        }
    }

    fn fatal_pending_signal(&self, cooperative: bool) -> Option<libc::c_int> {
        if self.interactive {
            return None;
        }
        trap::TERMINATING
            .iter()
            .copied()
            .filter(|&signal| !(cooperative && signal == libc::SIGINT))
            .filter(|&signal| {
                sys::signal_name(signal).is_some_and(|name| !self.traps.contains_key(name))
            })
            .find(|&signal| signals::is_pending(signal))
    }

    pub(crate) fn started_background(&mut self, pid: i32, text: &str) {
        let id = self.jobs.add(None, pid, vec![pid], text, JobState::Running);
        if self.interactive {
            eprintln!("[{id}] {pid}");
        }
    }

    pub(crate) fn collect_children(&mut self) {
        if !self.substitution_pids.is_empty() {
            self.substitution_pids
                .retain(|&pid| matches!(sys::waitpid(pid, libc::WNOHANG), Ok(None)));
        }
        if self.jobs.has_running() {
            self.jobs.reap();
        }
    }

    pub(crate) fn reap_jobs(&mut self) {
        self.collect_children();
        if self.interactive {
            for notice in self.jobs.take_finished() {
                eprintln!("{notice}");
            }
        }
    }

    pub(crate) fn run_lastpipe(&mut self, commands: &[Command], text: &str) -> i32 {
        let Some((last, first)) = commands.split_last() else {
            return 0;
        };
        let mut pipes = Vec::with_capacity(first.len());
        for _ in first {
            match sys::pipe() {
                Ok(pipe) => pipes.push(pipe),
                Err(error) => {
                    close_pipes(&pipes);
                    shell::error(format!("pipe: {}", sys::error_message(&error)));
                    return 1;
                }
            }
        }
        let mut pids = Vec::with_capacity(first.len());
        for (index, command) in first.iter().enumerate() {
            match sys::fork() {
                Ok(0) => {
                    self.enter_child(None, true);
                    if index > 0 {
                        let _ = sys::dup2(pipes[index - 1].0, 0);
                    }
                    let _ = sys::dup2(pipes[index].1, 1);
                    close_pipes(&pipes);
                    self.pipe_to_table = table::consumes_tables(&commands[index + 1]);
                    let status = self.run_command(command, true);
                    self.exit_child(status)
                }
                Ok(pid) => pids.push(pid),
                Err(error) => {
                    shell::error(format!("fork: {}", sys::error_message(&error)));
                    break;
                }
            }
        }
        let reader = pipes.last().map_or(-1, |pipe| pipe.0);
        let saved = vec![(0, sys::save_fd(0))];
        let _ = sys::dup2(reader, 0);
        close_pipes(&pipes);
        let last_status = self.run_command(last, false);
        sys::restore_fds(saved);
        let pgid = pids.first().copied().unwrap_or(0);
        self.wait_foreground(pgid, pids, text);
        self.pipe_statuses.push(last_status);
        if self.options.pipefail {
            self.pipe_statuses
                .iter()
                .rev()
                .copied()
                .find(|status| *status != 0)
                .unwrap_or(0)
        } else {
            last_status
        }
    }

    pub(crate) fn wait_until(&mut self, finished: impl Fn(&Self) -> bool) -> Option<libc::c_int> {
        let watch_children = !self.traps.contains_key("CHLD");
        if watch_children {
            signals::set(libc::SIGCHLD, signals::Disposition::Catch);
        }
        let interrupted = loop {
            let previous = sys::block_signals();
            if watch_children {
                signals::take(libc::SIGCHLD);
            }
            self.collect_children();
            if finished(self) {
                sys::set_signal_mask(&previous);
                break None;
            }
            if signals::any_pending() {
                sys::set_signal_mask(&previous);
                let signal = self.handle_signals();
                if signal.is_some() || self.flow != Flow::Normal {
                    break Some(signal.unwrap_or(libc::SIGINT));
                }
                continue;
            }
            sys::suspend(&previous);
            sys::set_signal_mask(&previous);
        };
        if watch_children {
            signals::set(libc::SIGCHLD, signals::Disposition::Default);
        }
        interrupted
    }
}

fn close_pipes(pipes: &[(i32, i32)]) {
    for &(read_end, write_end) in pipes {
        sys::close(read_end);
        sys::close(write_end);
    }
}

fn is_binary_file(path: &str) -> bool {
    use std::io::Read;
    let mut buffer = [0u8; 80];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let count = file.read(&mut buffer).unwrap_or(0);
    let head = &buffer[..count];
    let line = head
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(head, |end| &head[..end]);
    line.contains(&0)
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
