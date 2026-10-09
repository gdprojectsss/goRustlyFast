mod budget;

use std::collections::BTreeMap;
use std::io::Write;
use std::time::Instant;

use crate::sys;
use crate::table::json::Json;

pub(crate) use budget::{line_limit, save_full_output, trim};

const ENVIRONMENT: &[(&str, &str)] = &[
    ("GRFSH_AGENT", "1"),
    ("PAGER", "cat"),
    ("GIT_PAGER", "cat"),
    ("MANPAGER", "cat"),
    ("SYSTEMD_PAGER", ""),
    ("GIT_TERMINAL_PROMPT", "0"),
    ("GIT_EDITOR", "true"),
    ("DEBIAN_FRONTEND", "noninteractive"),
    ("PIP_NO_INPUT", "1"),
    ("NO_COLOR", "1"),
    ("CLICOLOR", "0"),
];

pub(crate) fn requested() -> bool {
    std::env::var("GRFSH_AGENT").is_ok_and(|value| !value.is_empty() && value != "0")
}

pub(crate) fn prepare_environment() {
    for (name, value) in ENVIRONMENT {
        unsafe { std::env::set_var(name, value) };
    }
    if sys::isatty(0)
        && let Ok(fd) = sys::open("/dev/null", libc::O_RDONLY)
    {
        let _ = sys::dup2(fd, 0);
        sys::close(fd);
    }
}

pub(crate) fn detach_from_terminal() {
    unsafe {
        libc::setsid();
    }
}

pub(crate) fn job_log_path() -> Option<std::path::PathBuf> {
    budget::output_directory().map(|directory| {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        directory.join(format!("job-{stamp}.log"))
    })
}

pub(crate) fn send_output_to(path: &std::path::Path) {
    let Some(path) = path.to_str() else {
        return;
    };
    if let Ok(fd) = sys::open(path, libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC) {
        unsafe { libc::fchmod(fd, 0o600) };
        let _ = sys::dup2(fd, 1);
        let _ = sys::dup2(fd, 2);
        sys::close(fd);
    }
}

pub(crate) struct Capture {
    saved_out: Option<i32>,
    saved_err: Option<i32>,
    out: i32,
    err: Option<i32>,
    started: Instant,
    environment: BTreeMap<String, String>,
}

pub(crate) struct Outcome {
    pub stdout: String,
    pub stderr: String,
    pub milliseconds: u128,
    pub environment: Vec<(String, Option<String>)>,
}

fn environment() -> BTreeMap<String, String> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

fn read_back(fd: i32) -> String {
    unsafe { libc::lseek(fd, 0, libc::SEEK_SET) };
    let bytes = sys::read_all(fd);
    sys::close(fd);
    String::from_utf8_lossy(&bytes).into_owned()
}

impl Capture {
    pub(crate) fn begin(separate: bool) -> std::io::Result<Capture> {
        let _ = std::io::stdout().flush();
        let out = sys::temp_file_with(b"")?;
        let err = if separate {
            Some(sys::temp_file_with(b"")?)
        } else {
            None
        };
        let saved_out = sys::save_fd(1);
        let saved_err = sys::save_fd(2);
        sys::dup2(out, 1)?;
        sys::dup2(err.unwrap_or(out), 2)?;
        Ok(Capture {
            saved_out,
            saved_err,
            out,
            err,
            started: Instant::now(),
            environment: environment(),
        })
    }

    pub(crate) fn finish(self) -> Outcome {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        sys::restore_fds(vec![(1, self.saved_out), (2, self.saved_err)]);
        let before = self.environment;
        let after = environment();
        let mut changes: Vec<(String, Option<String>)> = after
            .iter()
            .filter(|(name, value)| before.get(*name) != Some(value))
            .map(|(name, value)| (name.clone(), Some(value.clone())))
            .collect();
        changes.extend(
            before
                .keys()
                .filter(|name| !after.contains_key(*name))
                .map(|name| (name.clone(), None)),
        );
        changes.retain(|(name, _)| name != "PWD" && name != "OLDPWD" && name != "_");
        Outcome {
            stdout: read_back(self.out),
            stderr: self.err.map(read_back).unwrap_or_default(),
            milliseconds: self.started.elapsed().as_millis(),
            environment: changes,
        }
    }
}

struct Stream {
    text: String,
    omitted_lines: usize,
    log: Option<String>,
}

fn budgeted(text: &str, label: &str) -> Stream {
    let trimmed = trim(text, line_limit());
    let log = (trimmed.text != text)
        .then(|| save_full_output(text, label))
        .flatten()
        .map(|path| path.display().to_string());
    Stream {
        text: trimmed.text,
        omitted_lines: trimmed.omitted_lines,
        log,
    }
}

pub(crate) fn print_plain(outcome: &Outcome) {
    let stream = budgeted(&outcome.stdout, "output");
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(stream.text.as_bytes());
    if let Some(log) = stream.log {
        if !stream.text.ends_with('\n') && !stream.text.is_empty() {
            let _ = out.write_all(b"\n");
        }
        let _ = writeln!(out, "[grfsh: full output saved to {log}]");
    }
    let _ = out.flush();
}

pub(crate) fn result_json(outcome: &Outcome, status: i32, directory: &str) -> Json {
    let number = |value: u128| Json::Number(value.to_string());
    let text = |value: &str| Json::String(value.to_string());
    let stdout = budgeted(&outcome.stdout, "stdout");
    let stderr = budgeted(&outcome.stderr, "stderr");
    let log = |stream: &Stream| stream.log.as_deref().map_or(Json::Null, text);
    Json::Object(vec![
        ("status".into(), Json::Number(status.to_string())),
        ("stdout".into(), text(&stdout.text)),
        ("stderr".into(), text(&stderr.text)),
        ("duration_ms".into(), number(outcome.milliseconds)),
        ("cwd".into(), text(directory)),
        (
            "env".into(),
            Json::Object(
                outcome
                    .environment
                    .iter()
                    .map(|(name, value)| (name.clone(), value.as_deref().map_or(Json::Null, text)))
                    .collect(),
            ),
        ),
        (
            "truncated".into(),
            Json::Bool(
                stdout.omitted_lines + stderr.omitted_lines > 0
                    || stdout.log.is_some()
                    || stderr.log.is_some(),
            ),
        ),
        ("stdout_log".into(), log(&stdout)),
        ("stderr_log".into(), log(&stderr)),
    ])
}
