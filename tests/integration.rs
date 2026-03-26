use std::process::{Command, Stdio};

fn run(input: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HOME", "/tmp/grfsh_test_home")
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(input.as_bytes()).ok();
            child.wait_with_output()
        })
        .expect("failed to run grfsh");

    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn echo_basic() {
    assert_eq!(run("echo hello\n").trim(), "hello");
}

#[test]
fn echo_multiple_args() {
    assert_eq!(run("echo one two three\n").trim(), "one two three");
}

#[test]
fn echo_flag_n() {
    let out = run("echo -n no_newline\n");
    assert_eq!(out, "no_newline");
}

#[test]
fn pipe_simple() {
    assert_eq!(run("echo hello | tr h H\n").trim(), "Hello");
}

#[test]
fn pipe_chain() {
    assert_eq!(run("echo abc | tr a x | tr b y\n").trim(), "xyc");
}

#[test]
fn redirect_stdout() {
    let out = run("echo test > /tmp/grfsh_int_test.txt\ncat /tmp/grfsh_int_test.txt\n");
    assert_eq!(out.trim(), "test");
    let _ = std::fs::remove_file("/tmp/grfsh_int_test.txt");
}

#[test]
fn redirect_append() {
    let out = run(
        "echo first > /tmp/grfsh_append.txt\necho second >> /tmp/grfsh_append.txt\ncat /tmp/grfsh_append.txt\n",
    );
    assert_eq!(out.trim(), "first\nsecond");
    let _ = std::fs::remove_file("/tmp/grfsh_append.txt");
}

#[test]
fn redirect_stdin() {
    let out = run("echo content > /tmp/grfsh_stdin.txt\ncat < /tmp/grfsh_stdin.txt\n");
    assert_eq!(out.trim(), "content");
    let _ = std::fs::remove_file("/tmp/grfsh_stdin.txt");
}

#[test]
fn and_chain_success() {
    assert_eq!(run("echo a && echo b\n").trim(), "a\nb");
}

#[test]
fn and_chain_failure() {
    assert_eq!(run("false && echo nope\n").trim(), "");
}

#[test]
fn or_chain_success() {
    assert_eq!(run("true || echo nope\n").trim(), "");
}

#[test]
fn or_chain_failure() {
    assert_eq!(run("false || echo yes\n").trim(), "yes");
}

#[test]
fn semicolon_chain() {
    assert_eq!(run("echo a; echo b; echo c\n").trim(), "a\nb\nc");
}

#[test]
fn mixed_logic() {
    assert_eq!(run("false && echo no || echo yes\n").trim(), "yes");
}

#[test]
fn variable_expansion() {
    let out = run("export FOO=bar\necho $FOO\n");
    assert_eq!(out.trim(), "bar");
}

#[test]
fn single_quotes_literal() {
    assert_eq!(run("echo '$HOME'\n").trim(), "$HOME");
}

#[test]
fn double_quotes_expand_var() {
    let out = run("export X=val\necho \"$X\"\n");
    assert_eq!(out.trim(), "val");
}

#[test]
fn command_substitution_dollar() {
    assert_eq!(run("echo $(echo inner)\n").trim(), "inner");
}

#[test]
fn command_substitution_backtick() {
    assert_eq!(run("echo `echo back`\n").trim(), "back");
}

#[test]
fn command_substitution_nested() {
    assert_eq!(run("echo $(echo $(echo deep))\n").trim(), "deep");
}

#[test]
fn inline_comment() {
    assert_eq!(run("echo visible # hidden\n").trim(), "visible");
}

#[test]
fn cd_changes_dir() {
    let out = run("cd /tmp\npwd\n");
    assert_eq!(out.trim(), "/tmp");
}

#[test]
fn cd_minus() {
    let out = run("cd /tmp\ncd /var\ncd -\npwd\n");
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines.last().unwrap(), &"/tmp");
}

#[test]
fn alias_and_unalias() {
    let out = run("alias ll='echo aliased'\nll\nunalias ll\n");
    assert!(out.contains("aliased"));
}

#[test]
fn pipe_with_redirect() {
    let out = run("echo hello world | tr h H > /tmp/grfsh_pipe_redir.txt\ncat /tmp/grfsh_pipe_redir.txt\n");
    assert_eq!(out.trim(), "Hello world");
    let _ = std::fs::remove_file("/tmp/grfsh_pipe_redir.txt");
}

#[test]
fn glob_expansion() {
    let out = run("echo src/*.rs\n");
    assert!(out.contains("src/main.rs"));
    assert!(out.contains("src/parser.rs"));
}

#[test]
fn quoted_glob_no_expand() {
    assert_eq!(run("echo '*.rs'\n").trim(), "*.rs");
}

#[test]
fn export_and_echo() {
    let out = run("export GRFSH_TEST=works\necho $GRFSH_TEST\n");
    assert_eq!(out.trim(), "works");
}
