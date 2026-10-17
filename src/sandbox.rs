use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::shell;

const FORKS_PER_RUN: u32 = 16;
const TIME_PER_RUN: Duration = Duration::from_millis(1000);
const CHILD_SECONDS: u32 = 1;
const WATCHDOG_TICK: Duration = Duration::from_millis(5);
const ROOT_PREFIX: &str = "grfsh-sandbox-";

const ALLOWED_COMMANDS: &[&str] = &[
    "cd",
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
    "wait",
    "read",
    "set",
    "test",
    "[",
    "printf",
    "pushd",
    "popd",
    "dirs",
    "declare",
    "typeset",
    "readonly",
    "getopts",
    "shopt",
    "mapfile",
    "readarray",
    "help",
    "complete",
    "abbr",
    "bindkey",
    "let",
    "builtin",
    "caller",
    "times",
    "disown",
    "compgen",
    "enable",
    "umask",
    "source",
    ".",
];
const PSEUDO_SIGNALS: &[&str] = &["EXIT", "ERR", "DEBUG", "RETURN", "0"];
const DEVICES: &[&str] = &["/dev/null", "/dev/stdin", "/dev/stdout", "/dev/stderr"];

static ENABLED: AtomicBool = AtomicBool::new(false);
static IN_CHILD: AtomicBool = AtomicBool::new(false);
static EXPIRED: AtomicBool = AtomicBool::new(false);
static FORKS_LEFT: AtomicU32 = AtomicU32::new(0);
static DEADLINE: AtomicU64 = AtomicU64::new(0);
static CHILDREN: Mutex<Vec<libc::pid_t>> = Mutex::new(Vec::new());
static ROOT: OnceLock<PathBuf> = OnceLock::new();
static EPOCH: OnceLock<Instant> = OnceLock::new();

pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

pub(crate) fn enable(root: &Path) {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if ROOT.set(root).is_err() || ENABLED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(watchdog);
}

pub(crate) fn root() -> Option<&'static Path> {
    ROOT.get().map(PathBuf::as_path)
}

pub(crate) fn begin_run() {
    EXPIRED.store(false, Ordering::SeqCst);
    FORKS_LEFT.store(FORKS_PER_RUN, Ordering::SeqCst);
    DEADLINE.store(
        now_millis() + TIME_PER_RUN.as_millis() as u64,
        Ordering::SeqCst,
    );
}

pub(crate) fn end_run() {
    DEADLINE.store(0, Ordering::SeqCst);
    FORKS_LEFT.store(0, Ordering::SeqCst);
    let children = std::mem::take(&mut *lock_children());
    for pid in children {
        stop_unreaped_child(pid);
    }
    crate::signals::clear();
}

pub(crate) fn expired() -> bool {
    enabled() && EXPIRED.load(Ordering::SeqCst)
}

pub(crate) fn before_fork() -> io::Result<()> {
    if !enabled() {
        return Ok(());
    }
    let exhausted = IN_CHILD.load(Ordering::SeqCst)
        || EXPIRED.load(Ordering::SeqCst)
        || FORKS_LEFT.load(Ordering::SeqCst) == 0;
    if exhausted {
        return Err(io::Error::from_raw_os_error(libc::EAGAIN));
    }
    FORKS_LEFT.fetch_sub(1, Ordering::SeqCst);
    Ok(())
}

pub(crate) fn after_fork(pid: libc::pid_t) {
    if !enabled() {
        return;
    }
    if pid == 0 {
        IN_CHILD.store(true, Ordering::SeqCst);
        unsafe {
            libc::signal(libc::SIGALRM, libc::SIG_DFL);
            libc::alarm(CHILD_SECONDS);
        }
    } else if pid > 0 {
        lock_children().push(pid);
    }
}

pub(crate) fn check_exec() -> io::Result<()> {
    if enabled() {
        return Err(denied());
    }
    Ok(())
}

pub(crate) fn check_kill(pid: libc::pid_t) -> io::Result<()> {
    if !enabled() {
        return Ok(());
    }
    let own_child = pid > 0 && !IN_CHILD.load(Ordering::SeqCst) && lock_children().contains(&pid);
    if own_child {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(libc::EPERM))
    }
}

pub(crate) fn check_open(path: &str, flags: libc::c_int) -> io::Result<()> {
    if !enabled() || is_device(path) {
        return Ok(());
    }
    let writing = libc::O_WRONLY | libc::O_RDWR | libc::O_CREAT | libc::O_TRUNC | libc::O_APPEND;
    if flags & writing != 0 {
        return if inside_root(path) {
            Ok(())
        } else {
            Err(denied())
        };
    }
    check_read(path)
}

pub(crate) fn check_read(path: &str) -> io::Result<()> {
    if !enabled() || is_device(path) {
        return Ok(());
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() || metadata.is_dir() => Ok(()),
        Ok(_) => Err(denied()),
        Err(error) => Err(error),
    }
}

pub(crate) fn refuses(argv: &[String], function_defined: bool) -> bool {
    if !enabled() || function_defined || allows(argv) {
        return false;
    }
    shell::error(format!("{}: not allowed in the sandbox", argv[0]));
    true
}

struct Harness {
    environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    null: i32,
    saved_outputs: [i32; 2],
}

static HARNESS: OnceLock<Harness> = OnceLock::new();

pub(crate) fn run_untrusted(data: &[u8]) {
    let harness = HARNESS.get_or_init(prepare_harness);
    let Some(root) = root() else {
        return;
    };
    reset_process(harness, root);
    begin_run();
    let text = crate::bytes::decode(data);
    let mut lines = text.lines().map(String::from);
    let mut shell = crate::shell::Shell::new();
    shell.run_lines(|| lines.next());
    shell.flow = crate::shell::Flow::Normal;
    shell.run_exit_trap();
    drop(shell);
    end_run();
    reset_process(harness, root);
}

fn prepare_harness() -> Harness {
    remove_abandoned_roots();
    let root = std::env::temp_dir().join(format!("{ROOT_PREFIX}{}", std::process::id()));
    let _ = std::fs::create_dir_all(&root);
    for name in ["HOME", "TMPDIR", "XDG_CONFIG_HOME"] {
        unsafe { std::env::set_var(name, &root) };
    }
    unsafe { std::env::set_var("PATH", "") };
    enable(&root);
    let null = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    let saved_outputs = [1, 2].map(|fd| unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 100) });
    Harness {
        environment: std::env::vars_os().collect(),
        null,
        saved_outputs,
    }
}

fn remove_abandoned_roots() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(pid) = name
            .strip_prefix(ROOT_PREFIX)
            .and_then(|pid| pid.parse::<libc::pid_t>().ok())
        else {
            continue;
        };
        let alive = unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        if !alive {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn reset_process(harness: &Harness, root: &Path) {
    unsafe {
        libc::dup2(harness.null, 0);
        for (fd, saved) in [1, 2].into_iter().zip(harness.saved_outputs) {
            libc::dup2(if saved >= 0 { saved } else { harness.null }, fd);
        }
    }
    for (name, _) in std::env::vars_os() {
        unsafe { std::env::remove_var(name) };
    }
    for (name, value) in &harness.environment {
        unsafe { std::env::set_var(name, value) };
    }
    let _ = std::env::set_current_dir(root);
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let _ = std::fs::remove_dir_all(&path).or_else(|_| std::fs::remove_file(&path));
        }
    }
}

fn allows(argv: &[String]) -> bool {
    let name = argv[0].as_str();
    match name {
        "exec" => argv.len() == 1,
        "trap" => traps_only_pseudo_signals(&argv[1..]),
        _ => ALLOWED_COMMANDS.contains(&name),
    }
}

fn traps_only_pseudo_signals(args: &[String]) -> bool {
    let operands: Vec<&String> = args
        .iter()
        .skip_while(|argument| argument.starts_with('-') && argument.as_str() != "-")
        .collect();
    operands.iter().skip(1).all(|signal| {
        let upper = signal.to_ascii_uppercase();
        PSEUDO_SIGNALS.contains(&upper.strip_prefix("SIG").unwrap_or(&upper))
    })
}

fn is_device(path: &str) -> bool {
    DEVICES.contains(&path) || path.starts_with("/dev/fd/")
}

fn inside_root(path: &str) -> bool {
    let Some(root) = root() else {
        return false;
    };
    let path = Path::new(path);
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return false;
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(directory) => directory.join(path),
            Err(_) => return false,
        }
    };
    absolute.starts_with(root)
}

fn denied() -> io::Error {
    io::Error::from_raw_os_error(libc::EACCES)
}

fn lock_children() -> std::sync::MutexGuard<'static, Vec<libc::pid_t>> {
    CHILDREN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn stop_unreaped_child(pid: libc::pid_t) {
    let mut status = 0;
    let state = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if state == 0 {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            libc::waitpid(pid, &mut status, 0);
        }
    }
}

fn now_millis() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

fn watchdog() {
    loop {
        std::thread::sleep(WATCHDOG_TICK);
        let deadline = DEADLINE.load(Ordering::SeqCst);
        if deadline != 0 && now_millis() > deadline {
            EXPIRED.store(true, Ordering::SeqCst);
            crate::signals::mark_pending();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(String::from).collect()
    }

    #[test]
    fn only_safe_commands_are_allowed() {
        assert!(allows(&words("echo hi")));
        assert!(allows(&words("exec")));
        assert!(!allows(&words("exec ls")));
        assert!(!allows(&words("write-file /etc/passwd")));
        assert!(!allows(&words("kill -9 -1")));
        assert!(!allows(&words("ls")));
        assert!(!allows(&words("/bin/rm -rf x")));
    }

    #[test]
    fn traps_on_real_signals_are_refused() {
        assert!(allows(&words("trap echo EXIT")));
        assert!(allows(&words("trap -- echo ERR SIGRETURN")));
        assert!(allows(&words("trap -p")));
        assert!(!allows(&words("trap echo ALRM")));
        assert!(!allows(&words("trap echo EXIT 14")));
    }
}
