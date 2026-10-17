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
    let c_path = CString::new(crate::bytes::encode(path))
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
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

pub fn read_to_end(fd: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if count > 0 {
            out.extend_from_slice(&buffer[..count as usize]);
        } else if count == 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return out;
        }
    }
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
    Some(crate::bytes::decode(&line))
}

pub fn isatty(fd: i32) -> bool {
    unsafe { libc::isatty(fd) == 1 }
}

pub fn getpid() -> Pid {
    unsafe { libc::getpid() }
}

pub fn parent_pid() -> Pid {
    unsafe { libc::getppid() }
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
    signals::release_kept_ignored();
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
    signals::set(libc::SIGPIPE, signals::Disposition::Ignore);
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

fn wait_once(pid: Pid, flags: libc::c_int) -> io::Result<Option<(Pid, WaitResult)>> {
    let mut raw_status = 0;
    let result = unsafe { libc::waitpid(pid, &mut raw_status, flags) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if result == 0 {
        return Ok(None);
    }
    let outcome = if libc::WIFEXITED(raw_status) {
        WaitResult::Exited(libc::WEXITSTATUS(raw_status))
    } else if libc::WIFSIGNALED(raw_status) {
        WaitResult::Signaled(libc::WTERMSIG(raw_status))
    } else if libc::WIFSTOPPED(raw_status) {
        WaitResult::Stopped
    } else {
        WaitResult::Continued
    };
    Ok(Some((result, outcome)))
}

pub fn waitpid(pid: Pid, flags: libc::c_int) -> io::Result<Option<WaitResult>> {
    loop {
        match wait_once(pid, flags) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            other => return other.map(|found| found.map(|(_, result)| result)),
        }
    }
}

pub fn waitpid_interruptible(pid: Pid, flags: libc::c_int) -> io::Result<Option<WaitResult>> {
    wait_once(pid, flags).map(|found| found.map(|(_, result)| result))
}

pub fn block_signals() -> libc::sigset_t {
    unsafe {
        let mut all: libc::sigset_t = std::mem::zeroed();
        let mut previous: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut all);
        libc::sigprocmask(libc::SIG_BLOCK, &all, &mut previous);
        previous
    }
}

pub fn set_signal_mask(mask: &libc::sigset_t) {
    unsafe {
        libc::sigprocmask(libc::SIG_SETMASK, mask, std::ptr::null_mut());
    }
}

pub fn suspend(mask: &libc::sigset_t) {
    unsafe {
        libc::sigsuspend(mask);
    }
}

pub fn die_by_signal(signal: libc::c_int) -> ! {
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    signals::set(signal, signals::Disposition::Default);
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, signal);
        libc::sigprocmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
        libc::kill(libc::getpid(), signal);
        libc::_exit(128 + signal)
    }
}

pub fn default_pipe_signal() {
    signals::set(libc::SIGPIPE, signals::Disposition::Default);
}

pub fn signal_description(signal: libc::c_int) -> String {
    let text = unsafe { libc::strsignal(signal) };
    if text.is_null() {
        return format!("Signal {signal}");
    }
    unsafe { std::ffi::CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

pub fn process_times() -> [f64; 4] {
    let seconds = |time: libc::timeval| time.tv_sec as f64 + time.tv_usec as f64 / 1_000_000.0;
    let mut times = [0.0; 4];
    for (index, who) in [libc::RUSAGE_SELF, libc::RUSAGE_CHILDREN]
        .into_iter()
        .enumerate()
    {
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(who, &mut usage) } == 0 {
            times[index * 2] = seconds(usage.ru_utime);
            times[index * 2 + 1] = seconds(usage.ru_stime);
        }
    }
    times
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
        signals::set(signal, signals::Disposition::Default);
    }
}

fn ignored_on_entry(signal: libc::c_int) -> bool {
    unsafe {
        let mut current: libc::sigaction = std::mem::zeroed();
        libc::sigaction(signal, std::ptr::null(), &mut current);
        current.sa_sigaction == libc::SIG_IGN
    }
}

pub fn init_shell_signals(agent: bool) {
    for signal in [
        libc::SIGHUP,
        libc::SIGINT,
        libc::SIGTERM,
        libc::SIGALRM,
        libc::SIGUSR1,
        libc::SIGUSR2,
    ] {
        if ignored_on_entry(signal) {
            signals::keep_ignored(signal);
        }
    }
    signals::set(libc::SIGQUIT, signals::Disposition::Ignore);
    if !agent {
        default_pipe_signal();
    }
}

pub fn execv(path: &str, argv: &[String]) -> io::Error {
    let Ok(c_path) = CString::new(crate::bytes::encode(path)) else {
        return io::Error::from(io::ErrorKind::InvalidInput);
    };
    let c_args: Vec<CString> = argv
        .iter()
        .map(|argument| CString::new(crate::bytes::encode(argument)).unwrap_or_default())
        .collect();
    let mut pointers: Vec<*const libc::c_char> =
        c_args.iter().map(|argument| argument.as_ptr()).collect();
    pointers.push(std::ptr::null());
    unsafe { libc::execv(c_path.as_ptr(), pointers.as_ptr()) };
    io::Error::last_os_error()
}

pub fn signal_number(name: &str) -> Option<libc::c_int> {
    if let Ok(number) = name.parse() {
        let known = number == 0 || SIGNALS.iter().any(|&(_, signal)| signal == number);
        return known.then_some(number);
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
    ("ILL", libc::SIGILL),
    ("TRAP", libc::SIGTRAP),
    ("ABRT", libc::SIGABRT),
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    ("EMT", libc::SIGEMT),
    ("FPE", libc::SIGFPE),
    ("KILL", libc::SIGKILL),
    ("BUS", libc::SIGBUS),
    ("SEGV", libc::SIGSEGV),
    ("SYS", libc::SIGSYS),
    ("PIPE", libc::SIGPIPE),
    ("ALRM", libc::SIGALRM),
    ("TERM", libc::SIGTERM),
    ("URG", libc::SIGURG),
    ("STOP", libc::SIGSTOP),
    ("TSTP", libc::SIGTSTP),
    ("CONT", libc::SIGCONT),
    ("CHLD", libc::SIGCHLD),
    ("TTIN", libc::SIGTTIN),
    ("TTOU", libc::SIGTTOU),
    ("IO", libc::SIGIO),
    ("XCPU", libc::SIGXCPU),
    ("XFSZ", libc::SIGXFSZ),
    ("VTALRM", libc::SIGVTALRM),
    ("PROF", libc::SIGPROF),
    ("WINCH", libc::SIGWINCH),
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    ("INFO", libc::SIGINFO),
    #[cfg(target_os = "linux")]
    ("STKFLT", libc::SIGSTKFLT),
    #[cfg(target_os = "linux")]
    ("PWR", libc::SIGPWR),
    ("USR1", libc::SIGUSR1),
    ("USR2", libc::SIGUSR2),
];

pub fn signals_by_number() -> Vec<(&'static str, libc::c_int)> {
    let mut signals = SIGNALS.to_vec();
    signals.sort_by_key(|&(_, number)| number);
    signals
}

pub fn signal_listing() -> String {
    let signals = signals_by_number();
    let mut text = String::new();
    for (position, (name, number)) in signals.iter().enumerate() {
        text.push_str(&format!("{number:2}) SIG{name}"));
        text.push(if (position + 1).is_multiple_of(5) {
            '\n'
        } else {
            '\t'
        });
    }
    if !signals.len().is_multiple_of(5) {
        text.push('\n');
    }
    text
}

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

pub fn current_user_home() -> Option<String> {
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
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

pub fn user_id() -> u32 {
    unsafe { libc::getuid() }
}

pub fn effective_user_id() -> u32 {
    unsafe { libc::geteuid() }
}

pub fn group_ids() -> Vec<u32> {
    let mut groups = vec![0 as libc::gid_t; 1024];
    let count = unsafe { libc::getgroups(groups.len() as libc::c_int, groups.as_mut_ptr()) };
    groups.truncate(count.max(0) as usize);
    let primary = unsafe { libc::getgid() };
    let mut ordered = vec![primary];
    ordered.extend(groups.into_iter().filter(|group| *group != primary));
    ordered.dedup();
    ordered
}

pub fn kernel_release() -> String {
    let mut name: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut name) } != 0 {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(name.release.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

pub fn terminal_width(fd: i32) -> Option<usize> {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) };
    if result == 0 && size.ws_col > 0 {
        return Some(usize::from(size.ws_col));
    }
    std::env::var("COLUMNS").ok()?.parse().ok()
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

pub fn terminal_state(fd: i32) -> Option<libc::termios> {
    let mut state: libc::termios = unsafe { std::mem::zeroed() };
    (unsafe { libc::tcgetattr(fd, &mut state) } == 0).then_some(state)
}

pub fn terminal_size(fd: i32) -> Option<(usize, usize)> {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) };
    (result == 0 && size.ws_col > 0 && size.ws_row > 0)
        .then(|| (usize::from(size.ws_col), usize::from(size.ws_row)))
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
