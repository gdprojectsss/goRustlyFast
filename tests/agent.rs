use std::process::{Command, Output};

fn grfsh(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(args)
        .env_remove("GRFSH_AGENT")
        .env_remove("GRFSH_OUTPUT_LINES")
        .output()
        .expect("grfsh runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn json_result_separates_streams_and_reports_state() {
    let output = grfsh(&[
        "--json",
        "-c",
        "cd /; export AGENT_TEST=yes; echo out; echo err >&2; exit 3",
    ]);
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(3));
    assert!(text.starts_with("{\"status\":3,\"stdout\":\"out\\n\",\"stderr\":\"err\\n\""));
    assert!(text.contains("\"cwd\":\"/\""));
    assert!(text.contains("\"env\":{\"AGENT_TEST\":\"yes\"}"));
    assert!(text.contains("\"truncated\":false"));
}

#[test]
fn agent_mode_sets_non_interactive_defaults() {
    let output = grfsh(&[
        "--agent",
        "-c",
        "echo $PAGER $GIT_PAGER $NO_COLOR $GRFSH_AGENT",
    ]);
    assert_eq!(stdout(&output), "cat cat 1 1\n");
}

#[test]
fn long_output_is_cut_and_saved() {
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args([
            "--agent",
            "-c",
            "i=0; while (( i < 50 )); do echo line$i; (( i++ )); done",
        ])
        .env("GRFSH_OUTPUT_LINES", "10")
        .output()
        .expect("grfsh runs");
    let text = stdout(&output);
    assert!(
        text.starts_with("line0\nline1\nline2\nline3\nline4\n[grfsh: 40 lines omitted]\nline45\n")
    );
    let log = text
        .lines()
        .last()
        .and_then(|line| line.strip_prefix("[grfsh: full output saved to "))
        .and_then(|line| line.strip_suffix(']'))
        .expect("log path");
    let saved = std::fs::read_to_string(log).expect("log exists");
    assert_eq!(saved.lines().count(), 50);
    let _ = std::fs::remove_file(log);
}

#[test]
fn commands_cannot_open_the_terminal() {
    let started = std::time::Instant::now();
    let output = grfsh(&[
        "--agent",
        "-c",
        "sh -c ': < /dev/tty' 2>/dev/null; echo status=$?",
    ]);
    let text = stdout(&output);
    assert!(
        text.starts_with("status=") && text != "status=0\n",
        "{text}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn timeout_stops_the_whole_process_group() {
    let started = std::time::Instant::now();
    let output = grfsh(&[
        "-c",
        "timeout 0.3 sh -c 'sleep 20 & sleep 20'; echo status=$?",
    ]);
    assert_eq!(stdout(&output), "status=124\n");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn timeout_kill_after_and_functions() {
    let output = grfsh(&[
        "-c",
        "timeout -s INT -k 0.2 0.2 sh -c 'trap \"\" INT; sleep 20'; echo $?; f() { echo inside; }; timeout 5 f; timeout 5 false; echo $?",
    ]);
    assert_eq!(stdout(&output), "137\ninside\n1\n");
}

#[test]
fn background_jobs_write_to_a_log_in_agent_mode() {
    let output = grfsh(&["--agent", "-c", "echo from-job & wait"]);
    let text = stdout(&output);
    let path = text
        .lines()
        .find_map(|line| line.split("writes its output to ").nth(1))
        .expect("log notice");
    assert_eq!(std::fs::read_to_string(path).expect("log"), "from-job\n");
    let _ = std::fs::remove_file(path);
}

#[test]
fn replace_and_write_file_edit_atomically() {
    let directory = std::env::temp_dir().join(format!("grfsh_edit_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("notes/a.txt");
    let script = format!(
        "printf 'one\\ntwo\\ntwo\\n' | write-file -p {path}; replace {path} one ONE; replace {path} two TWO; echo status=$?; replace --all {path} two 2; cat {path}",
        path = file.display()
    );
    let output = grfsh(&["-c", &script]);
    assert_eq!(stdout(&output), "status=1\nONE\n2\n2\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("appears 2 times, at lines 2, 3"));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn explain_shows_expansions_without_running_anything() {
    let marker = std::env::temp_dir().join(format!("grfsh_explain_{}", std::process::id()));
    let script = format!(
        "dir=; rm -rf \"$dir\"/*; touch {marker}; echo $(touch {marker}) done; git reset --hard",
        marker = marker.display()
    );
    let output = grfsh(&["--explain", &script]);
    let text = stdout(&output);
    assert!(text.contains("warning: $dir is empty or unset here"));
    assert!(text.contains("warning: this deletes"));
    assert!(text.contains("is not run while explaining"));
    assert!(text.contains("git reset --hard discards uncommitted changes"));
    assert!(!marker.exists());
}

#[test]
fn explain_builtin_sees_current_variables_and_keeps_state() {
    let output = grfsh(&[
        "-c",
        "target=build; explain rm -r $target; cd /; explain cd /tmp; pwd",
    ]);
    let text = stdout(&output);
    assert!(text.contains("rm -r build"));
    assert!(text.ends_with("/\n"));
}

#[test]
fn search_respects_gitignore_and_prints_grep_style_text() {
    let directory = std::env::temp_dir().join(format!("grfsh_search_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join(".git")).unwrap();
    std::fs::create_dir_all(directory.join("build")).unwrap();
    std::fs::create_dir_all(directory.join("src")).unwrap();
    std::fs::write(directory.join(".gitignore"), "build/\n*.log\n!keep.log\n").unwrap();
    for file in ["build/out", "a.log", "keep.log", "src/main.rs", ".env"] {
        std::fs::write(directory.join(file), "x\nneedle here\n").unwrap();
    }
    std::fs::write(directory.join("src/data.bin"), b"needle\0binary").unwrap();
    let script = format!(
        "cd {}; search needle; search -l -g '*.rs' NEEDLE -i; search needle | count",
        directory.display()
    );
    let output = grfsh(&["-c", &script]);
    assert_eq!(
        stdout(&output),
        "keep.log:2:needle here\nsrc/main.rs:2:needle here\nsrc/main.rs\n2\n"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn search_results_pipe_into_replace() {
    let directory = std::env::temp_dir().join(format!("grfsh_pipe_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("a.txt"), "old one\nkeep old\nold two\n").unwrap();
    std::fs::write(directory.join("b.txt"), "old\n").unwrap();
    let script = format!(
        "cd {}; search '^old' a.txt | replace -n old new; search '^old' a.txt | replace old new; cat a.txt; search -l old . | replace old x; cat b.txt",
        directory.display()
    );
    let output = grfsh(&["-c", &script]);
    assert_eq!(
        stdout(&output),
        "a.txt:1\n- old one\n- keep old\n- old two\n+ new one\n+ keep old\n+ new two\n\
         a.txt: 2 replacements\nnew one\nkeep old\nnew two\na.txt: 1 replacement\nb.txt: 1 replacement\nx\n"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn serve_keeps_state_between_requests() {
    let socket = std::env::temp_dir().join(format!("grfsh-{}.sock", std::process::id()));
    let socket = socket.to_str().unwrap().to_string();
    struct Server(std::process::Child);
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_grfsh"))
            .args(["--serve", &socket])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    for _ in 0..100 {
        if std::path::Path::new(&socket).exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let first = grfsh(&[
        "--client",
        &socket,
        "cd /; name=kept; greet() { echo \"hi $name\"; }",
    ]);
    assert_eq!(first.status.code(), Some(0));
    let second = grfsh(&["--client", &socket, "greet; pwd"]);
    assert_eq!(stdout(&second), "hi kept\n/\n");
    let slow = grfsh(&["--client", &socket, "-t", "0.3", "while true; do :; done"]);
    assert_eq!(slow.status.code(), Some(124));
    let json = grfsh(&["--json", "--client", &socket, "echo out; false"]);
    assert!(stdout(&json).contains("\"status\":1,\"stdout\":\"out\\n\""));
    let _ = grfsh(&["--client", &socket, "exit 0"]);
    assert!(server.0.wait().unwrap().success());
    assert!(!std::path::Path::new(&socket).exists());
}

#[test]
fn write_file_refuses_empty_input_and_replace_handles_crlf() {
    let directory = std::env::temp_dir().join(format!("grfsh_edit_more_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("keep.txt");
    std::fs::write(&file, "one\r\ntwo\r\nthree\r\n").unwrap();
    let path = file.display();
    let empty = grfsh(&[
        "-c",
        &format!("write-file {path} < /dev/null; echo status=$?"),
    ]);
    assert_eq!(stdout(&empty), "status=1\n");
    assert!(String::from_utf8_lossy(&empty.stderr).contains("use -f"));
    let blocks = format!(
        "printf '<<<<<<<\\none\\ntwo\\n=======\\nuno\\n>>>>>>>\\n<<<<<<<\\nthree\\n=======\\n>>>>>>>\\n' | replace {path}; echo status=$?"
    );
    let replaced = grfsh(&["-c", &blocks]);
    assert_eq!(stdout(&replaced), "status=0\n", "{:?}", replaced);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "uno\r\n");
    let forced = grfsh(&[
        "-c",
        &format!("write-file -f {path} < /dev/null; echo status=$?"),
    ]);
    assert_eq!(stdout(&forced), "status=0\n");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn agent_mode_removes_day_old_output_logs() {
    let uid = String::from_utf8(Command::new("id").arg("-u").output().unwrap().stdout).unwrap();
    let directory = std::env::temp_dir().join(format!("grfsh-output-{}", uid.trim()));
    std::fs::create_dir_all(&directory).unwrap();
    let old = directory.join(format!("old-{}.log", std::process::id()));
    std::fs::write(&old, "x").unwrap();
    let status = Command::new("touch")
        .args(["-t", "202001010000"])
        .arg(&old)
        .status()
        .unwrap();
    assert!(status.success());
    let output = grfsh(&["--agent", "-c", "true"]);
    assert!(output.status.success());
    assert!(!old.exists());
}

#[test]
fn explain_keeps_substitutions_symbolic_and_follows_aliases() {
    let output = grfsh(&[
        "--explain",
        "x=$(date)\necho $x\nalias hello='echo hi'; hello\nhello there",
    ]);
    let text = stdout(&output);
    assert!(text.contains("   1  sets x=\"$(date)\""), "{text}");
    assert!(text.contains("   2  echo \"$(date)\""), "{text}");
    assert!(
        text.contains("aliases only take effect from the next line"),
        "{text}"
    );
    assert!(text.contains("   4  echo hi there"), "{text}");
}

struct Server {
    child: std::process::Child,
    socket: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn start_server(name: &str) -> Server {
    let socket = std::env::temp_dir().join(format!("grfsh-{name}-{}.sock", std::process::id()));
    let socket = socket.to_str().unwrap().to_string();
    let _ = std::fs::remove_file(&socket);
    let child = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(["--serve", &socket])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..200 {
        if std::path::Path::new(&socket).exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Server { child, socket }
}

#[test]
fn serve_requests_read_from_dev_null_and_timeouts_stop_read() {
    let server = start_server("stdin");
    let started = std::time::Instant::now();
    let read = grfsh(&[
        "--client",
        &server.socket,
        "-t",
        "5",
        "read x; echo status=$?",
    ]);
    assert_eq!(stdout(&read), "status=1\n");
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
    let fifo = std::env::temp_dir().join(format!("grfsh-fifo-{}", std::process::id()));
    let _ = std::fs::remove_file(&fifo);
    let command = format!("mkfifo {path}; read x <> {path}", path = fifo.display());
    let started = std::time::Instant::now();
    let blocked = grfsh(&["--client", &server.socket, "-t", "0.5", &command]);
    assert_eq!(blocked.status.code(), Some(124));
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("timed out after 0.5s"));
    let after = grfsh(&["--client", &server.socket, "echo still here"]);
    assert_eq!(stdout(&after), "still here\n");
    let _ = std::fs::remove_file(&fifo);
}

#[test]
fn serve_removes_its_socket_on_sigterm_and_sighup() {
    for signal in ["TERM", "HUP"] {
        let mut server = start_server(&format!("signal-{signal}"));
        let _ = Command::new("kill")
            .args([&format!("-{signal}"), &server.child.id().to_string()])
            .status();
        let status = server.child.wait().unwrap();
        assert!(!status.success());
        assert!(!std::path::Path::new(&server.socket).exists(), "{signal}");
    }
}

#[test]
fn serve_explains_socket_paths_that_are_too_long() {
    let long = format!("/tmp/{}.sock", "x".repeat(120));
    let output = grfsh(&["--serve", &long]);
    assert_eq!(output.status.code(), Some(2));
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("use a shorter path"), "{message}");
    assert!(!message.contains("SUN_LEN"));
}
