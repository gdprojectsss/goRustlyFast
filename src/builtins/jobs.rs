use std::io::Write;

use crate::jobs::{Job, JobState};
use crate::shell::{self, Flow, Shell};
use crate::sys;

#[derive(Default)]
struct ListOptions {
    pids_only: bool,
    long: bool,
    running: bool,
    stopped: bool,
    changed: bool,
}

fn parse_list_options(args: &[String]) -> Result<(ListOptions, &[String]), String> {
    let mut options = ListOptions::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        let Some(flags) = argument.strip_prefix('-').filter(|flags| !flags.is_empty()) else {
            break;
        };
        for flag in flags.chars() {
            match flag {
                'p' => options.pids_only = true,
                'l' => options.long = true,
                'r' => options.running = true,
                's' => options.stopped = true,
                'n' => options.changed = true,
                other => return Err(format!("jobs: -{other}: invalid option")),
            }
        }
        index += 1;
    }
    Ok((options, &args[index..]))
}

fn wanted(job: &Job, options: &ListOptions) -> bool {
    let state_matches = match (options.running, options.stopped) {
        (true, false) => job.state == JobState::Running,
        (false, true) => job.state == JobState::Stopped,
        _ => true,
    };
    state_matches && (!options.changed || job.state == JobState::Done)
}

fn print_job(out: &mut dyn Write, job: &Job, marker: char, options: &ListOptions) {
    if options.pids_only {
        let _ = writeln!(out, "{}", job.pgid);
    } else {
        let _ = writeln!(out, "{}", job.line(marker, options.long));
    }
}

pub(super) fn list(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let (options, specs) = match parse_list_options(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            shell::error(message);
            return 2;
        }
    };
    shell.collect_children();
    let mut status = 0;
    let mut shown = Vec::new();
    if specs.is_empty() {
        let inherited = shell.jobs.inherited().to_vec();
        for (position, job) in inherited.iter().enumerate() {
            let marker = match inherited.len() - position {
                1 => '+',
                2 => '-',
                _ => ' ',
            };
            if wanted(job, &options) {
                print_job(out, job, marker, &options);
            }
        }
        shown = shell.jobs.iter().map(|job| job.id).collect();
    } else {
        for spec in specs {
            match shell.jobs.find(Some(spec)) {
                Ok(index) => shown.push(shell.jobs.get(index).id),
                Err(error) => {
                    shell::error(format!("jobs: {error}"));
                    status = 1;
                }
            }
        }
    }
    let mut finished = Vec::new();
    for id in shown {
        let Some(job) = shell.jobs.iter().find(|job| job.id == id) else {
            continue;
        };
        if !wanted(job, &options) {
            continue;
        }
        print_job(out, job, shell.jobs.marker(id), &options);
        if job.state == JobState::Done {
            finished.push(id);
        }
    }
    for id in finished {
        if let Some(index) = shell.jobs.index_of(id) {
            shell.jobs.forget(index);
        }
    }
    status
}

fn job_index(shell: &Shell, command: &str, spec: Option<&String>) -> Option<usize> {
    let spec = spec.map(String::as_str);
    match shell.jobs.find(spec) {
        Ok(index) => Some(index),
        Err(_) if spec.is_none() => {
            shell::error(format!("{command}: current: no such job"));
            None
        }
        Err(error) => {
            shell::error(format!("{command}: {error}"));
            None
        }
    }
}

fn require_job_control(shell: &Shell, command: &str) -> bool {
    if !shell.interactive {
        shell::error(format!("{command}: no job control"));
    }
    shell.interactive
}

pub(super) fn fg(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    if !require_job_control(shell, "fg") {
        return 1;
    }
    shell.collect_children();
    let Some(index) = job_index(shell, "fg", args.first()) else {
        return 1;
    };
    if shell.jobs.get(index).state == JobState::Done {
        let job = shell.jobs.forget(index);
        let _ = writeln!(out, "{}", job.text);
        return job.status();
    }
    let job = shell.jobs.remove(index);
    let _ = writeln!(out, "{}", job.text);
    let _ = out.flush();
    sys::tcsetpgrp(job.pgid);
    let _ = sys::kill(-job.pgid, libc::SIGCONT);
    let running = job.running_pids();
    shell.wait_job(Some(job.id), job.pgid, running, &job.text)
}

pub(super) fn bg(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    if !require_job_control(shell, "bg") {
        return 1;
    }
    let Some(index) = job_index(shell, "bg", args.first()) else {
        return 1;
    };
    let job = shell.jobs.get_mut(index);
    if job.state == JobState::Done {
        shell::error(format!("bg: job {} has terminated", job.id));
        return 1;
    }
    job.state = JobState::Running;
    let _ = sys::kill(-job.pgid, libc::SIGCONT);
    let _ = writeln!(out, "[{}]+ {} &", job.id, job.text);
    0
}

struct WaitOptions {
    next: bool,
    variable: Option<String>,
    force: bool,
}

fn parse_wait_options(args: &[String]) -> Result<(WaitOptions, &[String]), String> {
    let mut options = WaitOptions {
        next: false,
        variable: None,
        force: false,
    };
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        let Some(flags) = argument.strip_prefix('-').filter(|flags| !flags.is_empty()) else {
            break;
        };
        index += 1;
        for (position, flag) in flags.char_indices() {
            match flag {
                'n' => options.next = true,
                'f' => options.force = true,
                'p' => {
                    let attached = &flags[position + 1..];
                    let name = if attached.is_empty() {
                        index += 1;
                        args.get(index - 1)
                            .cloned()
                            .ok_or("wait: -p: option requires an argument")?
                    } else {
                        attached.to_string()
                    };
                    options.variable = Some(name);
                    break;
                }
                other => return Err(format!("wait: -{other}: invalid option")),
            }
        }
    }
    Ok((options, &args[index..]))
}

enum Target {
    Job(usize),
    Pid(sys::Pid),
}

fn resolve_target(shell: &Shell, argument: &str) -> Result<Target, (String, i32)> {
    if argument.starts_with('%') {
        return shell
            .jobs
            .find(Some(argument))
            .map(|index| Target::Job(shell.jobs.get(index).id))
            .map_err(|error| (format!("wait: {error}"), 127));
    }
    match argument.parse::<sys::Pid>() {
        Ok(pid) if pid > 0 => Ok(Target::Pid(pid)),
        _ => Err((
            format!("wait: `{argument}': not a pid or valid job spec"),
            2,
        )),
    }
}

fn job_settled(shell: &Shell, id: usize, force: bool) -> bool {
    shell
        .jobs
        .iter()
        .find(|job| job.id == id)
        .is_none_or(|job| match job.state {
            JobState::Running => false,
            JobState::Stopped => !force,
            JobState::Done => true,
        })
}

fn interrupted(shell: &Shell, signal: libc::c_int) -> i32 {
    if shell.flow == Flow::Exit {
        shell.last_status
    } else {
        128 + signal
    }
}

fn set_reported(shell: &mut Shell, variable: &Option<String>, pid: Option<sys::Pid>) {
    if let Some(name) = variable {
        match pid {
            Some(pid) => shell.set_variable(name, &pid.to_string()),
            None => shell.unset_variable(name),
        }
    }
}

fn wait_job_id(shell: &mut Shell, id: usize, options: &WaitOptions) -> i32 {
    if let Some(signal) = shell.wait_until(|shell| job_settled(shell, id, options.force)) {
        return interrupted(shell, signal);
    }
    let Some(index) = shell.jobs.index_of(id) else {
        return 127;
    };
    let job = shell.jobs.get(index);
    set_reported(shell, &options.variable, Some(job.pgid));
    let job = shell.jobs.get(index);
    match job.state {
        JobState::Done => shell.jobs.forget(index).status(),
        _ => 128 + libc::SIGTSTP,
    }
}

fn wait_pid(shell: &mut Shell, pid: sys::Pid, options: &WaitOptions) -> i32 {
    if let Some(index) = shell.jobs.find_pid(pid) {
        let id = shell.jobs.get(index).id;
        let pid_done = |shell: &Shell| {
            shell.jobs.find_pid(pid).is_none_or(|index| {
                let job = shell.jobs.get(index);
                !job.running_pids().contains(&pid) || job_settled(shell, id, options.force)
            })
        };
        if let Some(signal) = shell.wait_until(pid_done) {
            return interrupted(shell, signal);
        }
        set_reported(shell, &options.variable, Some(pid));
        let Some(index) = shell.jobs.find_pid(pid) else {
            return shell.jobs.remembered_status(pid).unwrap_or(127);
        };
        let job = shell.jobs.get(index);
        let position = job.pids.iter().position(|&known| known == pid);
        let result = position.and_then(|position| job.results[position]);
        if job.state == JobState::Done {
            shell.jobs.forget(index);
        }
        return result.map_or(128 + libc::SIGTSTP, sys::WaitResult::status);
    }
    if let Some(status) = shell.jobs.remembered_status(pid) {
        set_reported(shell, &options.variable, Some(pid));
        return status;
    }
    if let Some(position) = shell
        .substitution_pids
        .iter()
        .position(|&known| known == pid)
    {
        shell.substitution_pids.remove(position);
        set_reported(shell, &options.variable, Some(pid));
        return sys::waitpid(pid, 0)
            .ok()
            .flatten()
            .map_or(127, sys::WaitResult::status);
    }
    shell::error(format!("wait: pid {pid} is not a child of this shell"));
    127
}

fn finished_candidate(shell: &Shell, candidates: &[Target]) -> Option<usize> {
    shell.jobs.iter().position(|job| {
        job.state == JobState::Done
            && (candidates.is_empty()
                || candidates.iter().any(|target| match target {
                    Target::Job(id) => job.id == *id,
                    Target::Pid(pid) => job.pids.contains(pid),
                }))
    })
}

fn wait_next(shell: &mut Shell, candidates: &[Target], options: &WaitOptions) -> i32 {
    loop {
        if let Some(index) = finished_candidate(shell, candidates) {
            let job = shell.jobs.forget(index);
            set_reported(shell, &options.variable, Some(job.pgid));
            return job.status();
        }
        let waitable = shell.jobs.iter().any(|job| {
            job.state == JobState::Running
                && (candidates.is_empty()
                    || candidates.iter().any(|target| match target {
                        Target::Job(id) => job.id == *id,
                        Target::Pid(pid) => job.pids.contains(pid),
                    }))
        });
        if !waitable {
            set_reported(shell, &options.variable, None);
            return 127;
        }
        let any_finished = |shell: &Shell| {
            finished_candidate(shell, candidates).is_some()
                || !shell.jobs.iter().any(|job| job.state == JobState::Running)
        };
        if let Some(signal) = shell.wait_until(any_finished) {
            return interrupted(shell, signal);
        }
    }
}

fn wait_all(shell: &mut Shell, options: &WaitOptions) -> i32 {
    let force = options.force;
    let settled = |shell: &Shell| {
        shell.jobs.iter().all(|job| match job.state {
            JobState::Running => false,
            JobState::Stopped => !force,
            JobState::Done => true,
        })
    };
    if let Some(signal) = shell.wait_until(settled) {
        return interrupted(shell, signal);
    }
    while let Some(index) = shell.jobs.first_done() {
        shell.jobs.forget(index);
    }
    0
}

pub(super) fn wait(shell: &mut Shell, args: &[String]) -> i32 {
    let (options, arguments) = match parse_wait_options(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            shell::error(message);
            return 2;
        }
    };
    let mut targets = Vec::new();
    let mut status = 0;
    for argument in arguments {
        match resolve_target(shell, argument) {
            Ok(target) => targets.push(target),
            Err((message, code)) => {
                shell::error(message);
                status = code;
                if options.next {
                    return code;
                }
            }
        }
    }
    if options.next {
        return wait_next(shell, &targets, &options);
    }
    if arguments.is_empty() {
        return wait_all(shell, &options);
    }
    for target in targets {
        status = match target {
            Target::Job(id) => wait_job_id(shell, id, &options),
            Target::Pid(pid) => wait_pid(shell, pid, &options),
        };
        if shell.flow != Flow::Normal {
            break;
        }
    }
    status
}

fn signal_for_listing(argument: &str, out: &mut dyn Write) -> bool {
    if let Ok(number) = argument.parse::<i32>() {
        let number = if number > 128 { number - 128 } else { number };
        if let Some(name) = sys::signal_name(number) {
            let _ = writeln!(out, "{name}");
            return true;
        }
    } else if let Some(number) = sys::signal_number(argument).filter(|&number| number > 0) {
        let _ = writeln!(out, "{number}");
        return true;
    }
    shell::error(format!("kill: {argument}: invalid signal specification"));
    false
}

fn list_signals(args: &[String], out: &mut dyn Write) -> i32 {
    if args.is_empty() {
        let _ = write!(out, "{}", sys::signal_listing());
        return 0;
    }
    let mut status = 0;
    for argument in args {
        if !signal_for_listing(argument, out) {
            status = 1;
        }
    }
    status
}

fn parse_signal(text: &str) -> Result<libc::c_int, String> {
    sys::signal_number(text).ok_or_else(|| format!("kill: {text}: invalid signal specification"))
}

pub(super) fn kill(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    let mut signal = libc::SIGTERM;
    let mut rest = args;
    match rest.first().map(String::as_str) {
        Some("-l") | Some("-L") => return list_signals(&rest[1..], out),
        Some("-s") | Some("-n") => {
            let Some(name) = rest.get(1) else {
                shell::error(format!("kill: {}: option requires an argument", rest[0]));
                return 2;
            };
            match parse_signal(name) {
                Ok(number) => signal = number,
                Err(message) => {
                    shell::error(message);
                    return 1;
                }
            }
            rest = &rest[2..];
        }
        Some("--") => rest = &rest[1..],
        Some(flag) if flag.starts_with('-') && flag.len() > 1 => {
            match parse_signal(&flag[1..]) {
                Ok(number) => signal = number,
                Err(message) => {
                    shell::error(message);
                    return 1;
                }
            }
            rest = &rest[1..];
        }
        _ => {}
    }
    if rest.first().is_some_and(|argument| argument == "--") {
        rest = &rest[1..];
    }
    if rest.is_empty() {
        shell::error(
            "kill: usage: kill [-s sigspec | -n signum | -sigspec] pid | jobspec ... or kill -l [sigspec]",
        );
        return 2;
    }

    shell.collect_children();
    let mut status = 0;
    for target in rest {
        let (pid, stopped) = if target.starts_with('%') {
            match job_index(shell, "kill", Some(target)) {
                Some(index) => {
                    let job = shell.jobs.get(index);
                    (-job.pgid, job.state == JobState::Stopped)
                }
                None => {
                    status = 1;
                    continue;
                }
            }
        } else {
            match target.parse() {
                Ok(pid) => (pid, false),
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
            shell::error(format!("kill: ({target}) - {}", sys::error_message(&error)));
            status = 1;
        } else if stopped && signal != libc::SIGCONT {
            let _ = sys::kill(pid, libc::SIGCONT);
        }
    }
    status
}

struct DisownOptions {
    keep: bool,
    all: bool,
    running: bool,
}

pub(super) fn disown(shell: &mut Shell, args: &[String]) -> i32 {
    let mut options = DisownOptions {
        keep: false,
        all: false,
        running: false,
    };
    let mut index = 0;
    while let Some(flags) = args
        .get(index)
        .and_then(|argument| argument.strip_prefix('-'))
        .filter(|flags| !flags.is_empty())
    {
        index += 1;
        if flags == "-" {
            break;
        }
        for flag in flags.chars() {
            match flag {
                'h' => options.keep = true,
                'a' => options.all = true,
                'r' => options.running = true,
                other => {
                    shell::error(format!("disown: -{other}: invalid option"));
                    return 2;
                }
            }
        }
    }
    shell.collect_children();
    let specs = &args[index..];
    let mut ids = Vec::new();
    let mut status = 0;
    if specs.is_empty() && (options.all || options.running) {
        ids = shell
            .jobs
            .iter()
            .filter(|job| !options.running || job.state == JobState::Running)
            .map(|job| job.id)
            .collect();
    } else if specs.is_empty() {
        match job_index(shell, "disown", None) {
            Some(index) => ids.push(shell.jobs.get(index).id),
            None => return 1,
        }
    } else {
        for spec in specs {
            let found = match spec.parse::<sys::Pid>() {
                Ok(pid) if !spec.starts_with('%') => shell.jobs.find_pid(pid),
                _ => shell.jobs.find(Some(spec)).ok(),
            };
            match found {
                Some(index) => ids.push(shell.jobs.get(index).id),
                None => {
                    shell::error(format!("disown: {spec}: no such job"));
                    status = 1;
                }
            }
        }
    }
    for id in ids {
        let Some(index) = shell.jobs.index_of(id) else {
            continue;
        };
        if options.keep {
            shell.jobs.get_mut(index).keep_on_hangup = true;
        } else {
            shell.jobs.remove(index);
        }
    }
    status
}

pub(super) fn times(out: &mut dyn Write) -> i32 {
    let format = |seconds: f64| format!("{}m{:.3}s", (seconds / 60.0) as u64, seconds % 60.0);
    let [user, system, children_user, children_system] = sys::process_times();
    let _ = writeln!(out, "{} {}", format(user), format(system));
    let _ = writeln!(out, "{} {}", format(children_user), format(children_system));
    0
}
