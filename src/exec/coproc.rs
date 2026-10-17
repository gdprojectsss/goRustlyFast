use std::rc::Rc;

use crate::shell::{self, Shell};
use crate::syntax::ast::Command;
use crate::sys;
use crate::variables::Array;

fn close_all(fds: &[i32]) {
    for &fd in fds {
        sys::close(fd);
    }
}

fn moved_high(fd: i32) -> i32 {
    match sys::save_fd(fd) {
        Some(high) => {
            sys::close(fd);
            high
        }
        None => fd,
    }
}

impl Shell {
    pub(crate) fn run_coproc(&mut self, name: &str, body: &Rc<Command>) -> i32 {
        let (to_child_read, to_child_write) = match sys::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                shell::error(format!("coproc: pipe: {}", sys::error_message(&error)));
                return 1;
            }
        };
        let (from_child_read, from_child_write) = match sys::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                close_all(&[to_child_read, to_child_write]);
                shell::error(format!("coproc: pipe: {}", sys::error_message(&error)));
                return 1;
            }
        };
        let pipes = [
            to_child_read,
            to_child_write,
            from_child_read,
            from_child_write,
        ];
        match sys::fork() {
            Ok(0) => {
                self.enter_child(Some(0), false);
                let _ = sys::dup2(to_child_read, 0);
                let _ = sys::dup2(from_child_write, 1);
                close_all(&pipes);
                let status = self.run_command(body, true);
                sys::exit_now(status)
            }
            Ok(pid) => {
                sys::setpgid(pid, pid);
                close_all(&[to_child_read, from_child_write]);
                let read_fd = moved_high(from_child_read);
                let write_fd = moved_high(to_child_write);
                let fds = Array::indexed(vec![read_fd.to_string(), write_fd.to_string()]);
                if let Err(error) = self.set_array(name, fds) {
                    shell::error(format!("coproc: {error}"));
                }
                self.set_variable(&format!("{name}_PID"), &pid.to_string());
                self.started_background(pid, &format!("coproc {name}"));
                self.last_background_pid = Some(pid);
                0
            }
            Err(error) => {
                close_all(&pipes);
                shell::error(format!("coproc: fork: {}", sys::error_message(&error)));
                1
            }
        }
    }
}
