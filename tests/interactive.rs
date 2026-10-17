use std::ffi::CString;
use std::time::{Duration, Instant};

struct Terminal {
    fd: i32,
    pid: libc::pid_t,
    output: String,
}

fn private_home(name: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("grfsh_pty_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    home
}

impl Terminal {
    fn start(environment: &[(&str, &str)]) -> Terminal {
        let home = std::env::temp_dir().join(format!("grfsh_pty_home_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&home);
        Terminal::start_at(&home, environment)
    }

    fn start_at(home: &std::path::Path, environment: &[(&str, &str)]) -> Terminal {
        let program = CString::new(env!("CARGO_BIN_EXE_grfsh")).unwrap();
        let mut variables: Vec<CString> = vec![
            CString::new(format!("HOME={}", home.display())).unwrap(),
            CString::new(format!(
                "PATH={}",
                std::env::var("PATH").unwrap_or_default()
            ))
            .unwrap(),
            CString::new("TERM=xterm-256color").unwrap(),
            CString::new("GRFSH_PROMPT=$ ").unwrap(),
        ];
        variables.extend(
            environment
                .iter()
                .map(|(name, value)| CString::new(format!("{name}={value}")).unwrap()),
        );
        let mut size = libc::winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let mut fd = 0;
        let pid = unsafe {
            libc::forkpty(
                &mut fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut size,
            )
        };
        assert!(pid >= 0, "forkpty failed");
        if pid == 0 {
            let arguments = [program.as_ptr(), std::ptr::null()];
            let mut environment: Vec<*const libc::c_char> =
                variables.iter().map(|variable| variable.as_ptr()).collect();
            environment.push(std::ptr::null());
            unsafe {
                libc::execve(program.as_ptr(), arguments.as_ptr(), environment.as_ptr());
                libc::_exit(127);
            }
        }
        let mut terminal = Terminal {
            fd,
            pid,
            output: String::new(),
        };
        terminal.wait_for("\x1b[?2004h");
        terminal
    }

    fn send(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let written = unsafe { libc::write(self.fd, bytes.as_ptr().cast(), bytes.len()) };
        assert_eq!(written, bytes.len() as isize);
    }

    fn read_some(&mut self, timeout: Duration) {
        let mut poll = libc::pollfd {
            fd: self.fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, timeout.as_millis() as libc::c_int) };
        if ready <= 0 {
            return;
        }
        let mut buffer = [0u8; 4096];
        let count = unsafe { libc::read(self.fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if count > 0 {
            self.output
                .push_str(&String::from_utf8_lossy(&buffer[..count as usize]));
        }
    }

    fn pump(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.read_some(Duration::from_millis(20));
        }
    }

    fn exit_status(&mut self, timeout: Duration) -> Option<i32> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let mut status = 0;
            let result = unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) };
            if result == self.pid {
                return Some(if libc::WIFEXITED(status) {
                    libc::WEXITSTATUS(status)
                } else {
                    128 + libc::WTERMSIG(status)
                });
            }
            self.read_some(Duration::from_millis(20));
        }
        None
    }

    fn wait_for(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(found) = self.output.find(needle) {
                let end = found + needle.len();
                let seen = self.output[..end].to_string();
                self.output.drain(..end);
                return seen;
            }
            self.read_some(Duration::from_millis(50));
        }
        panic!(
            "timed out waiting for {needle:?}, terminal showed:\n{}",
            self.output
        );
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        unsafe {
            libc::kill(self.pid, libc::SIGKILL);
            libc::close(self.fd);
            libc::waitpid(self.pid, std::ptr::null_mut(), 0);
        }
    }
}

#[test]
fn ctrl_c_stops_a_builtin_loop() {
    let mut terminal = Terminal::start(&[]);
    terminal.send("while true; do :; done\r");
    terminal.pump(Duration::from_millis(400));
    terminal.send("\x03");
    terminal.wait_for("^C");
    terminal.send("echo status=$?\r");
    terminal.wait_for("status=130");
}

#[test]
fn ctrl_z_stops_and_fg_resumes() {
    let mut terminal = Terminal::start(&[]);
    terminal.send("sleep 30\r");
    terminal.pump(Duration::from_millis(400));
    terminal.send("\x1a");
    terminal.wait_for("Stopped");
    terminal.pump(Duration::from_millis(200));
    terminal.send("jobs\r");
    terminal.wait_for(&format!("{:<27}sleep 30", "Stopped"));
    terminal.send("fg\r");
    terminal.wait_for("sleep 30\r\n");
    terminal.pump(Duration::from_millis(400));
    terminal.send("\x03");
    terminal.wait_for("^C");
    terminal.send("echo after=$?\r");
    terminal.wait_for("after=130");
}

#[test]
fn ctrl_c_interrupts_wait() {
    let mut terminal = Terminal::start(&[]);
    terminal.send("sleep 3 &\r");
    terminal.pump(Duration::from_millis(300));
    terminal.send("wait\r");
    terminal.pump(Duration::from_millis(400));
    terminal.send("\x03");
    terminal.send("echo waited=$?\r");
    terminal.wait_for("waited=130");
    terminal.send("kill %1\r");
    terminal.pump(Duration::from_millis(200));
}

#[test]
fn multiline_input_waits_for_the_end_of_a_loop() {
    let mut terminal = Terminal::start(&[]);
    terminal.send("for i in 1 2\r");
    terminal.send("do echo item$i\r");
    terminal.send("done\r");
    terminal.wait_for("item1");
    terminal.wait_for("item2");
}

#[test]
fn transient_prompt_collapses_after_enter() {
    let mut terminal = Terminal::start(&[("GRFSH_THEME", "pure"), ("GRFSH_PROMPT", "")]);
    terminal.send("echo hi\r");
    let seen = terminal.wait_for("hi\r\n");
    assert!(
        seen.contains("\x1b[2A"),
        "no cursor-up sequence in {seen:?}"
    );
}

#[test]
fn tab_completes_builtins() {
    let mut terminal = Terminal::start(&[]);
    terminal.send("configure-pr\t");
    terminal.wait_for("configure-prompt");
}

#[test]
fn lines_arriving_together_all_run() {
    let home = private_home("burst");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("echo burst-A\recho burst-B\recho burst-C\r");
    terminal.wait_for("burst-A\r\n");
    terminal.wait_for("burst-B\r\n");
    terminal.wait_for("burst-C\r\n");
}

#[test]
fn ctrl_c_in_read_keeps_the_next_line() {
    let home = private_home("read_interrupt");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("read answer\r");
    terminal.pump(Duration::from_millis(300));
    terminal.send("\x03");
    terminal.pump(Duration::from_millis(300));
    terminal.send("echo kept-$((40 + 2))\r");
    terminal.wait_for("kept-42");
}

#[test]
fn ctrl_d_warns_about_stopped_jobs_first() {
    let home = private_home("ctrl_d");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("sleep 30\r");
    terminal.pump(Duration::from_millis(300));
    terminal.send("\x1a");
    terminal.wait_for("Stopped");
    terminal.pump(Duration::from_millis(200));
    terminal.send("\x04");
    terminal.wait_for("exit");
    terminal.wait_for("unfinished jobs");
    assert_eq!(terminal.exit_status(Duration::from_millis(300)), None);
    terminal.send("\x04");
    assert!(terminal.exit_status(Duration::from_secs(5)).is_some());
}

#[test]
fn ctrl_z_leaves_a_loop_and_kill_is_reported() {
    let home = private_home("ctrl_z_loop");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("while true; do sleep 1; done\r");
    terminal.pump(Duration::from_millis(400));
    terminal.send("\x1a");
    terminal.wait_for("Stopped");
    terminal.send("echo back-$?\r");
    terminal.wait_for(&format!("back-{}", 128 + libc::SIGTSTP));
    terminal.send("kill -9 %1\r");
    terminal.pump(Duration::from_millis(300));
    terminal.send("\r");
    terminal.wait_for("Killed");
}

#[test]
fn terminal_modes_come_back_after_a_killed_command() {
    let home = private_home("termios");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("sh -c 'stty raw -echo; kill -9 $$'\r");
    terminal.pump(Duration::from_millis(500));
    terminal.send("stty -a | grep -c -- -icanon\r");
    terminal.wait_for("\r\n0\r\n");
}

#[test]
fn hangup_at_the_prompt_exits_and_terminate_is_ignored() {
    let home = private_home("hangup");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("echo ready\r");
    terminal.wait_for("ready\r\n");
    terminal.pump(Duration::from_millis(200));
    unsafe { libc::kill(terminal.pid, libc::SIGTERM) };
    assert_eq!(terminal.exit_status(Duration::from_millis(500)), None);
    unsafe { libc::kill(terminal.pid, libc::SIGHUP) };
    assert_eq!(
        terminal.exit_status(Duration::from_secs(5)),
        Some(128 + libc::SIGHUP)
    );
}

#[test]
fn history_is_appended_as_commands_run() {
    let home = private_home("history_append");
    let old: String = (0..300).map(|index| format!("echo old{index}\n")).collect();
    std::fs::write(home.join(".grfsh_history"), format!("#V2\n{old}")).unwrap();
    let mut first = Terminal::start_at(&home, &[]);
    let mut second = Terminal::start_at(&home, &[]);
    first.send("echo from-first\r");
    first.wait_for("from-first\r\n");
    second.send("echo from-second\r");
    second.wait_for("from-second\r\n");
    first.pump(Duration::from_millis(200));
    let saved = std::fs::read_to_string(home.join(".grfsh_history")).unwrap();
    assert!(saved.contains("echo old0\n"), "history was truncated");
    assert!(saved.contains("echo from-first\n"));
    assert!(saved.contains("echo from-second\n"));
}

#[test]
fn history_expansion_and_fc() {
    let home = private_home("fc");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("echo one two three\r");
    terminal.wait_for("one two three\r\n");
    terminal.send("echo !!:2-3\r");
    terminal.wait_for("two three\r\n");
    terminal.send("^two^2\r");
    terminal.wait_for("2 three\r\n");
    terminal.send("fc -l -2\r");
    terminal.wait_for("2\t echo two three");
    terminal.wait_for("3\t echo 2 three");
    terminal.send("fc -s three=3 echo\r");
    terminal.wait_for("2 3\r\n");
    terminal.send("history 2\r");
    terminal.wait_for("echo 2 3");
}

#[test]
fn fc_runs_the_edited_command() {
    let home = private_home("fc_edit");
    let editor = home.join("editor.sh");
    std::fs::write(
        &editor,
        "#!/bin/sh\nsed 's/before/after/' \"$1\" > \"$1.new\" && mv \"$1.new\" \"$1\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&editor, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let editor_text = editor.display().to_string();
    let mut terminal = Terminal::start_at(&home, &[("FCEDIT", &editor_text)]);
    terminal.send("echo before\r");
    terminal.wait_for("before\r\n");
    terminal.send("fc\r");
    terminal.wait_for("echo after\r\n");
    terminal.wait_for("after\r\n");
}

#[test]
fn completion_lists_candidates_and_expands_variables() {
    let home = private_home("completion");
    std::fs::create_dir_all(home.join("projects")).unwrap();
    std::fs::create_dir_all(home.join("programs")).unwrap();
    std::fs::create_dir_all(home.join(".hidden")).unwrap();
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("cd ~/pro\t\t");
    terminal.wait_for("programs/  projects/");
    terminal.send("\x15echo $HOME/proj\t\r");
    terminal.wait_for("/projects/\r\n");
    terminal.send("\x15cd ~/\t\t");
    let listing = terminal.wait_for("projects/");
    assert!(!listing.contains(".hidden"), "{listing:?}");
}

#[test]
fn ctrl_c_clears_the_suggestion_and_sets_status() {
    let home = private_home("suggestion_interrupt");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("echo suggestion-here\r");
    terminal.wait_for("suggestion-here\r\n");
    terminal.send("echo sugg");
    terminal.pump(Duration::from_millis(300));
    terminal.send("\x03");
    let seen = terminal.wait_for("^C");
    assert!(
        seen.ends_with("echo sugg^C") || seen.contains("\x1b[K"),
        "{seen:?}"
    );
    terminal.send("echo status-$?\r");
    terminal.wait_for("status-130");
}

#[test]
fn suggestion_keys_and_last_argument() {
    let home = private_home("suggestion_keys");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("echo accepted-by-ctrl-e\r");
    terminal.wait_for("accepted-by-ctrl-e\r\n");
    terminal.send("echo acc\x05\r");
    terminal.wait_for("\r\naccepted-by-ctrl-e\r\n");
    terminal.send("echo first /tmp/last-word\r");
    terminal.wait_for("last-word\r\n");
    terminal.send("echo yanked=\x1b.\r");
    terminal.wait_for("yanked=/tmp/last-word\r\n");
}

#[test]
fn window_size_variables() {
    let home = private_home("window_size");
    let mut terminal = Terminal::start_at(&home, &[]);
    terminal.send("echo size=$COLUMNS,$LINES\r");
    terminal.wait_for("size=100,30");
}
