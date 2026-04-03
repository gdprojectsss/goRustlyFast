use std::io::Write;

use crate::jobs::JobState;
use crate::shell::{self, Shell};
use crate::sys;

pub(super) fn list(shell: &mut Shell, out: &mut dyn Write) -> i32 {
    shell.reap_jobs();
    let current = shell.jobs.current_id();
    for job in shell.jobs.iter() {
        let marker = if Some(job.id) == current { '+' } else { ' ' };
        let state = match job.state {
            JobState::Running => "Running",
            JobState::Stopped => "Stopped",
        };
        let _ = writeln!(out, "[{}]{marker}  {state:<10} {}", job.id, job.text);
    }
    0
}

fn job_index(shell: &Shell, command: &str, spec: Option<&String>) -> Option<usize> {
    match shell.jobs.find(spec.map(String::as_str)) {
        Ok(index) => Some(index),
        Err(error) => {
            shell::error(format!("{command}: {error}"));
            None
        }
    }
}

pub(super) fn fg(shell: &mut Shell, args: &[String]) -> i32 {
    let Some(index) = job_index(shell, "fg", args.first()) else {
        return 1;
    };
    let job = shell.jobs.remove(index);
    println!("{}", job.text);
    if shell.interactive {
        sys::tcsetpgrp(job.pgid);
    }
    let _ = sys::kill(-job.pgid, libc::SIGCONT);
    shell.wait_job(Some(job.id), job.pgid, job.pids, &job.text)
}

pub(super) fn bg(shell: &mut Shell, args: &[String]) -> i32 {
    let Some(index) = job_index(shell, "bg", args.first()) else {
        return 1;
    };
    let job = shell.jobs.get_mut(index);
    job.state = JobState::Running;
    let _ = sys::kill(-job.pgid, libc::SIGCONT);
    println!("[{}]+ {} &", job.id, job.text);
    0
}

pub(super) fn wait(shell: &mut Shell, args: &[String]) -> i32 {
    let mut status = 0;
    let targets: Vec<usize> = if args.is_empty() {
        shell.jobs.iter().map(|job| job.id).collect()
    } else {
        let mut job_ids = Vec::new();
        for argument in args {
            let Some(index) = job_index(shell, "wait", Some(argument)) else {
                return 127;
            };
            job_ids.push(shell.jobs.iter().nth(index).map_or(0, |job| job.id));
        }
        job_ids
    };
    for id in targets {
        let Some(index) = shell.jobs.iter().position(|job| job.id == id) else {
            continue;
        };
        let job = shell.jobs.remove(index);
        for pid in job.pids {
            if let Ok(Some(result)) = sys::waitpid(pid, 0) {
                status = result.status();
            }
        }
    }
    status
}

pub(super) fn kill(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let mut signal = libc::SIGTERM;
    let mut rest = args;
    match rest.first().map(String::as_str) {
        Some("-l") => {
            let names: Vec<_> = sys::SIGNALS.iter().map(|(name, _)| *name).collect();
            let _ = writeln!(out, "{}", names.join(" "));
            return 0;
        }
        Some("-s") | Some("-n") if rest.len() > 1 => {
            match sys::signal_number(&rest[1]) {
                Some(number) => signal = number,
                None => {
                    shell::error(format!("kill: {}: invalid signal", rest[1]));
                    return 1;
                }
            }
            rest = &rest[2..];
        }
        Some(flag) if flag.starts_with('-') && flag.len() > 1 => {
            match sys::signal_number(&flag[1..]) {
                Some(number) => signal = number,
                None => {
                    shell::error(format!("kill: {flag}: invalid signal"));
                    return 1;
                }
            }
            rest = &rest[1..];
        }
        _ => {}
    }
    if rest.is_empty() {
        shell::error("usage: kill [-signal] pid | %job ...");
        return 2;
    }

    let mut status = 0;
    for target in rest {
        let pid = if target.starts_with('%') {
            match job_index(shell, "kill", Some(target)) {
                Some(index) => -shell.jobs.iter().nth(index).map_or(0, |job| job.pgid),
                None => {
                    status = 1;
                    continue;
                }
            }
        } else {
            match target.parse() {
                Ok(pid) => pid,
                Err(_) => {
                    shell::error(format!(
                        "kill: {target}: arguments must be process or job IDs"
                    ));
                    status = 1;
                    continue;
                }
            }
        };
        if let Err(error) = sys::kill(pid, signal) {
            shell::error(format!("kill: {target}: {}", sys::error_message(&error)));
            status = 1;
        } else if pid < 0 && signal != libc::SIGCONT {
            let _ = sys::kill(pid, libc::SIGCONT);
        }
    }
    status
}
