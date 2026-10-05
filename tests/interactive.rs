use std::ffi::CString;
use std::time::{Duration, Instant};

struct Terminal {
    fd: i32,
    pid: libc::pid_t,
    output: String,
}

impl Terminal {
    fn start(environment: &[(&str, &str)]) -> Terminal {
        let home = std::env::temp_dir().join(format!("grfsh_pty_home_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&home);
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
    terminal.wait_for("Stopped    sleep 30");
    terminal.send("fg\r");
    terminal.wait_for("sleep 30\r\n");
    terminal.pump(Duration::from_millis(400));
    terminal.send("\x03");
    terminal.wait_for("^C");
    terminal.send("echo after=$?\r");
    terminal.wait_for("after=130");
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
