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
