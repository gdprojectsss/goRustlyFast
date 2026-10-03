use std::ffi::CString;
use std::io::{self, Write};

use crate::signals;

pub type Pid = libc::pid_t;

fn check(result: libc::c_int) -> io::Result<libc::c_int> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

pub fn error_message(error: &io::Error) -> String {
    let text = error.to_string();
    match text.find(" (os error") {
        Some(index) => text[..index].to_string(),
        None => text,
    }
}

pub fn fork() -> io::Result<Pid> {
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    check(unsafe { libc::fork() })
}

pub fn pipe() -> io::Result<(i32, i32)> {
    let mut descriptors = [0; 2];
    check(unsafe { libc::pipe(descriptors.as_mut_ptr()) })?;
    set_cloexec(descriptors[0]);
    set_cloexec(descriptors[1]);
    Ok((descriptors[0], descriptors[1]))
}

pub fn clear_cloexec(fd: i32) {
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, 0);
    }
}

pub fn set_cloexec(fd: i32) {
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
}

pub fn dup2(source: i32, destination: i32) -> io::Result<()> {
    check(unsafe { libc::dup2(source, destination) }).map(|_| ())
}

pub fn close(fd: i32) {
    unsafe {
        libc::close(fd);
    }
}

pub fn save_fd(fd: i32) -> Option<i32> {
    let saved = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10) };
    (saved >= 0).then_some(saved)
}

pub fn restore_fds(saved: Vec<(i32, Option<i32>)>) {
    let _ = io::stdout().flush();
    for (fd, copy) in saved.into_iter().rev() {
        match copy {
            Some(copy) => {
                let _ = dup2(copy, fd);
                close(copy);
            }
            None => close(fd),
        }
    }
}

pub fn open(path: &str, flags: libc::c_int) -> io::Result<i32> {
    let c_path = CString::new(path).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    check(unsafe {
        libc::open(
            c_path.as_ptr(),
            flags | libc::O_CLOEXEC,
            0o666 as libc::c_uint,
        )
    })
}

pub fn temp_file_with(content: &[u8]) -> io::Result<i32> {
    let directory = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());
    let directory = directory.trim_end_matches('/');
    for attempt in 0..100 {
        let path = format!("{directory}/grfsh-{}-{attempt}", std::process::id());
        let fd = match open(&path, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL) {
            Ok(fd) => fd,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let _ = std::fs::remove_file(&path);
        write_all(fd, content)?;
        unsafe { libc::lseek(fd, 0, libc::SEEK_SET) };
        return Ok(fd);
    }
    Err(io::Error::from(io::ErrorKind::AlreadyExists))
}

pub fn write_all(fd: i32, mut buffer: &[u8]) -> io::Result<()> {
    while !buffer.is_empty() {
        let count = unsafe { libc::write(fd, buffer.as_ptr().cast(), buffer.len()) };
        if count < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        buffer = &buffer[count as usize..];
    }
    Ok(())
}

pub fn read(fd: i32, buffer: &mut [u8]) -> io::Result<usize> {
    loop {
        let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if count >= 0 {
            return Ok(count as usize);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted || signals::any_pending() {
            return Err(error);
        }
    }
}

pub fn read_all(fd: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buffer = [0u8; 8192];
    while let Ok(count) = read(fd, &mut buffer) {
        if count == 0 {
            break;
        }
        out.extend_from_slice(&buffer[..count]);
    }
    out
}

pub fn read_line(fd: i32) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match read(fd, &mut byte) {
            Ok(1) => {
                line.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            _ if line.is_empty() => return None,
            _ => break,
        }
    }
    Some(String::from_utf8_lossy(&line).into_owned())
}

pub fn isatty(fd: i32) -> bool {
    unsafe { libc::isatty(fd) == 1 }
}

pub fn getpid() -> Pid {
    unsafe { libc::getpid() }
}

pub fn setpgid(pid: Pid, pgid: Pid) {
    unsafe {
        libc::setpgid(pid, pgid);
    }
}

pub fn kill(pid: Pid, signal: libc::c_int) -> io::Result<()> {
    check(unsafe { libc::kill(pid, signal) }).map(|_| ())
}

pub fn init_job_control() -> Pid {
    signals::set(libc::SIGHUP, signals::Disposition::Catch);
    unsafe {
        loop {
            let pgrp = libc::getpgrp();
            if libc::tcgetpgrp(0) == pgrp {
                break;
            }
            libc::kill(-pgrp, libc::SIGTTIN);
        }
    }
    ignore_job_signals();
    let pid = getpid();
    setpgid(0, pid);
    tcsetpgrp(pid);
    pid
}

pub fn tcsetpgrp(pgid: Pid) {
    unsafe {
        libc::tcsetpgrp(0, pgid);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaitResult {
    Exited(i32),
    Signaled(i32),
    Stopped,
    Continued,
}

impl WaitResult {
    pub fn status(self) -> i32 {
        match self {
            WaitResult::Exited(code) => code,
            WaitResult::Signaled(signal) => 128 + signal,
            WaitResult::Stopped => 128 + libc::SIGTSTP,
            WaitResult::Continued => 0,
        }
    }
}

pub fn waitpid(pid: Pid, flags: libc::c_int) -> io::Result<Option<WaitResult>> {
    let mut raw_status = 0;
    loop {
        let result = unsafe { libc::waitpid(pid, &mut raw_status, flags) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if result == 0 {
            return Ok(None);
        }
        let result = if libc::WIFEXITED(raw_status) {
            WaitResult::Exited(libc::WEXITSTATUS(raw_status))
        } else if libc::WIFSIGNALED(raw_status) {
            WaitResult::Signaled(libc::WTERMSIG(raw_status))
        } else if libc::WIFSTOPPED(raw_status) {
            WaitResult::Stopped
        } else {
            WaitResult::Continued
        };
        return Ok(Some(result));
    }
}

pub fn waitpid_any() -> io::Result<Option<(Pid, WaitResult)>> {
    let mut raw_status = 0;
    loop {
        let result = unsafe { libc::waitpid(-1, &mut raw_status, 0) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted && !signals::any_pending() {
                continue;
            }
            return Err(error);
        }
        let outcome = if libc::WIFEXITED(raw_status) {
            WaitResult::Exited(libc::WEXITSTATUS(raw_status))
        } else if libc::WIFSIGNALED(raw_status) {
            WaitResult::Signaled(libc::WTERMSIG(raw_status))
        } else {
            WaitResult::Stopped
        };
        return Ok(Some((result, outcome)));
    }
}

pub fn exit_now(code: i32) -> ! {
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    unsafe { libc::_exit(code) }
}

const JOB_SIGNALS: [libc::c_int; 5] = [
    libc::SIGINT,
    libc::SIGQUIT,
    libc::SIGTSTP,
    libc::SIGTTIN,
    libc::SIGTTOU,
];

pub fn ignore_job_signals() {
    for signal in JOB_SIGNALS {
        let disposition = if signal == libc::SIGINT {
            signals::Disposition::Catch
        } else {
            signals::Disposition::Ignore
        };
        signals::set(signal, disposition);
    }
}

pub fn reset_child_signals() {
    for signal in JOB_SIGNALS.into_iter().chain([libc::SIGPIPE]) {
        unsafe {
            libc::signal(signal, libc::SIG_DFL);
        }
    }
}

pub fn execv(path: &str, argv: &[String]) -> io::Error {
    let Ok(c_path) = CString::new(path) else {
        return io::Error::from(io::ErrorKind::InvalidInput);
    };
    let c_args: Vec<CString> = argv
        .iter()
        .map(|argument| CString::new(argument.as_str()).unwrap_or_default())
        .collect();
    let mut pointers: Vec<*const libc::c_char> =
        c_args.iter().map(|argument| argument.as_ptr()).collect();
    pointers.push(std::ptr::null());
    unsafe { libc::execv(c_path.as_ptr(), pointers.as_ptr()) };
    io::Error::last_os_error()
}

pub fn signal_number(name: &str) -> Option<libc::c_int> {
    if let Ok(number) = name.parse() {
        return Some(number);
    }
    let name = name.to_ascii_uppercase();
    let name = name.strip_prefix("SIG").unwrap_or(&name);
    SIGNALS
        .iter()
        .find(|(signal_name, _)| *signal_name == name)
        .map(|&(_, number)| number)
}

pub fn signal_name(number: libc::c_int) -> Option<&'static str> {
    SIGNALS
        .iter()
        .find(|(_, signal)| *signal == number)
        .map(|&(name, _)| name)
}

pub const SIGNALS: &[(&str, libc::c_int)] = &[
    ("HUP", libc::SIGHUP),
    ("INT", libc::SIGINT),
    ("QUIT", libc::SIGQUIT),
    ("KILL", libc::SIGKILL),
    ("USR1", libc::SIGUSR1),
    ("USR2", libc::SIGUSR2),
    ("PIPE", libc::SIGPIPE),
    ("ALRM", libc::SIGALRM),
    ("TERM", libc::SIGTERM),
    ("CHLD", libc::SIGCHLD),
    ("CONT", libc::SIGCONT),
    ("STOP", libc::SIGSTOP),
    ("TSTP", libc::SIGTSTP),
    ("TTIN", libc::SIGTTIN),
    ("TTOU", libc::SIGTTOU),
    ("WINCH", libc::SIGWINCH),
];

pub fn user_home(user: &str) -> Option<String> {
    let c_path = CString::new(user).ok()?;
    unsafe {
        let entry = libc::getpwnam(c_path.as_ptr());
        if entry.is_null() || (*entry).pw_dir.is_null() {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr((*entry).pw_dir)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

pub fn hostname() -> Option<String> {
    let mut buffer = [0u8; 256];
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if result != 0 {
        return None;
    }
    let end = buffer
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

pub fn terminal_width(fd: i32) -> Option<usize> {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) };
    if result == 0 && size.ws_col > 0 {
        return Some(usize::from(size.ws_col));
    }
    std::env::var("COLUMNS").ok()?.parse().ok()
}

pub fn cpu_times() -> (f64, f64) {
    let seconds = |time: libc::timeval| time.tv_sec as f64 + time.tv_usec as f64 / 1_000_000.0;
    let mut user = 0.0;
    let mut system = 0.0;
    for who in [libc::RUSAGE_SELF, libc::RUSAGE_CHILDREN] {
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(who, &mut usage) } == 0 {
            user += seconds(usage.ru_utime);
            system += seconds(usage.ru_stime);
        }
    }
    (user, system)
}

pub fn wait_readable(fd: i32, timeout: std::time::Duration) -> bool {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let milliseconds = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
    loop {
        let result = unsafe { libc::poll(&mut poll, 1, milliseconds) };
        if result < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            if signals::any_pending() {
                return false;
            }
            continue;
        }
        return result > 0;
    }
}

pub fn disable_echo(fd: i32) -> Option<libc::termios> {
    unsafe {
        let mut saved: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut saved) != 0 {
            return None;
        }
        let mut quiet = saved;
        quiet.c_lflag &= !libc::ECHO;
        libc::tcsetattr(fd, libc::TCSANOW, &quiet);
        Some(saved)
    }
}

pub fn restore_terminal(fd: i32, saved: &libc::termios) {
    unsafe {
        libc::tcsetattr(fd, libc::TCSANOW, saved);
    }
}

pub fn write_private(path: &std::path::Path, contents: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(contents)
}

pub fn make_private(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

static SAVED_TERMINAL: std::sync::OnceLock<libc::termios> = std::sync::OnceLock::new();

pub fn remember_terminal() {
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(0, &mut saved) } == 0 {
        let _ = SAVED_TERMINAL.set(saved);
    }
}

pub fn restore_saved_terminal() {
    if let Some(saved) = SAVED_TERMINAL.get() {
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, saved);
        }
    }
}

pub fn unblock_signals() {
    unsafe {
        let mut empty: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut empty);
        libc::sigprocmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut());
    }
}
