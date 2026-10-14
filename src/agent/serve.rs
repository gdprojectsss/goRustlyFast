use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use super::{Capture, result_json};
use crate::shell::{self, Flow, Shell};
use crate::signals;
use crate::sys;
use crate::table::json::{self, Json};

static RUNNING: Mutex<Vec<i32>> = Mutex::new(Vec::new());
static GENERATION: AtomicU64 = AtomicU64::new(0);
static TIMED_OUT: AtomicBool = AtomicBool::new(false);

pub(crate) fn register(pids: &[i32]) {
    if let Ok(mut running) = RUNNING.lock() {
        running.extend_from_slice(pids);
    }
}

pub(crate) fn unregister(pids: &[i32]) {
    if let Ok(mut running) = RUNNING.lock() {
        running.retain(|pid| !pids.contains(pid));
    }
}

pub(crate) fn timed_out() -> bool {
    TIMED_OUT.load(Ordering::SeqCst)
}

fn signal_running(signal: libc::c_int) {
    let pids = RUNNING
        .lock()
        .map(|running| running.clone())
        .unwrap_or_default();
    for pid in pids {
        if sys::kill(-pid, signal).is_err() {
            let _ = sys::kill(pid, signal);
        }
    }
}

fn arm(limit: Duration) -> u64 {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(limit);
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        TIMED_OUT.store(true, Ordering::SeqCst);
        signals::mark_pending();
        signal_running(libc::SIGTERM);
        std::thread::sleep(Duration::from_secs(2));
        if GENERATION.load(Ordering::SeqCst) == generation {
            signal_running(libc::SIGKILL);
        }
    });
    generation
}

fn disarm() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    TIMED_OUT.store(false, Ordering::SeqCst);
}

struct Request {
    command: String,
    timeout: Option<Duration>,
    id: Json,
}

fn parse_request(line: &str) -> Result<Request, String> {
    if !line.trim_start().starts_with('{') {
        return Ok(Request {
            command: line.to_string(),
            timeout: None,
            id: Json::Null,
        });
    }
    let Json::Object(fields) = json::parse(line)? else {
        return Err("expected a JSON object".into());
    };
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    };
    let Some(Json::String(command)) = field("command") else {
        return Err("the request needs a \"command\" string".into());
    };
    let timeout = match field("timeout") {
        Some(Json::Number(seconds)) => {
            let seconds: f64 = seconds.parse().map_err(|_| "invalid timeout")?;
            (seconds > 0.0).then(|| Duration::from_secs_f64(seconds))
        }
        _ => None,
    };
    Ok(Request {
        command: command.clone(),
        timeout,
        id: field("id").cloned().unwrap_or(Json::Null),
    })
}

fn respond(shell: &mut Shell, request: Request) -> Json {
    let capture = match Capture::begin(true) {
        Ok(capture) => capture,
        Err(error) => {
            return Json::Object(vec![
                ("id".into(), request.id),
                ("error".into(), Json::String(sys::error_message(&error))),
            ]);
        }
    };
    if let Some(limit) = request.timeout {
        arm(limit);
    }
    shell.run_source(&request.command);
    let timed_out = TIMED_OUT.load(Ordering::SeqCst);
    disarm();
    if timed_out {
        shell.last_status = 124;
    }
    if !matches!(shell.flow, Flow::Normal | Flow::Exit) {
        shell.flow = Flow::Normal;
    }
    let outcome = capture.finish();
    let directory = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    crate::repl::log_command(
        &request.command,
        &directory,
        Duration::from_millis(outcome.milliseconds as u64),
        shell.last_status,
    );
    let mut result = result_json(&outcome, shell.last_status, &directory);
    if let Json::Object(fields) = &mut result {
        fields.insert(0, ("id".into(), request.id));
        fields.push(("timed_out".into(), Json::Bool(timed_out)));
        fields.push(("exited".into(), Json::Bool(shell.flow == Flow::Exit)));
    }
    result
}

fn handle(shell: &mut Shell, stream: UnixStream) {
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    for line in BufReader::new(reader).lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = match parse_request(&line) {
            Ok(request) => respond(shell, request),
            Err(message) => Json::Object(vec![("error".into(), Json::String(message))]),
        };
        if writeln!(writer, "{}", response.to_text()).is_err() || shell.flow == Flow::Exit {
            break;
        }
    }
}

pub(crate) fn serve(shell: &mut Shell, path: &str) -> i32 {
    let absolute = std::env::current_dir()
        .map(|directory| directory.join(path))
        .unwrap_or_else(|_| path.into());
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.file_type().is_socket() {
            shell::error(format!("--serve: {path} exists and is not a socket"));
            return 2;
        }
        if UnixStream::connect(path).is_ok() {
            shell::error(format!(
                "--serve: another grfsh is already serving on {path}"
            ));
            return 2;
        }
        let _ = std::fs::remove_file(path);
    }
    let previous = unsafe { libc::umask(0o077) };
    let listener = UnixListener::bind(path);
    unsafe { libc::umask(previous) };
    let listener = match listener {
        Ok(listener) => listener,
        Err(error) => {
            shell::error(format!("--serve: {path}: {}", sys::error_message(&error)));
            return 2;
        }
    };
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    eprintln!("grfsh: serving on {path}");
    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            handle(shell, stream);
        }
        if shell.flow == Flow::Exit {
            break;
        }
    }
    let _ = std::fs::remove_file(&absolute);
    shell.last_status
}

pub(crate) fn client(path: &str, args: &[String], raw: bool) -> i32 {
    let (timeout, words) = match args {
        [flag, seconds, rest @ ..] if flag == "-t" || flag == "--timeout" => {
            (Some(seconds.clone()), rest)
        }
        rest => (None, rest),
    };
    let words = match words {
        [flag, rest @ ..] if flag == "-c" => rest,
        rest => rest,
    };
    if words.is_empty() {
        shell::error("--client: usage: grfsh --client socket [-t seconds] command...");
        return 2;
    }
    let mut request = vec![("command".to_string(), Json::String(words.join(" ")))];
    if let Some(seconds) = timeout {
        request.push(("timeout".into(), Json::Number(seconds)));
    }
    let mut stream = match UnixStream::connect(path) {
        Ok(stream) => stream,
        Err(error) => {
            shell::error(format!("--client: {path}: {}", sys::error_message(&error)));
            return 2;
        }
    };
    if writeln!(stream, "{}", Json::Object(request).to_text()).is_err() {
        shell::error("--client: the server closed the connection");
        return 2;
    }
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).unwrap_or(0) == 0 {
        shell::error("--client: no response from the server");
        return 2;
    }
    if raw {
        print!("{line}");
    }
    let Ok(Json::Object(fields)) = json::parse(&line) else {
        shell::error("--client: the server sent an invalid response");
        return 2;
    };
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    };
    if let Some(Json::String(message)) = field("error") {
        shell::error(format!("--client: {message}"));
        return 2;
    }
    if !raw {
        if let Some(Json::String(text)) = field("stdout") {
            print!("{text}");
        }
        if let Some(Json::String(text)) = field("stderr") {
            eprint!("{text}");
        }
    }
    match field("status") {
        Some(Json::Number(status)) => status.parse().unwrap_or(1),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_plain_lines_or_json() {
        let plain = parse_request("echo hi").unwrap();
        assert_eq!(plain.command, "echo hi");
        let json = parse_request("{\"command\":\"ls\\nls\",\"timeout\":1.5,\"id\":7}").unwrap();
        assert_eq!(json.command, "ls\nls");
        assert_eq!(json.timeout, Some(Duration::from_millis(1500)));
        assert_eq!(json.id, Json::Number("7".into()));
        assert!(parse_request("{\"timeout\":1}").is_err());
    }
}
