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
    let output = grfsh(&[
        "--agent",
        "-c",
        "sh -c ': < /dev/tty' 2>/dev/null; echo status=$?",
    ]);
    assert_eq!(stdout(&output), "status=1\n");
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
