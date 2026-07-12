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
    let out = run(
        "echo hello world | tr h H > /tmp/grfsh_pipe_redir.txt\ncat /tmp/grfsh_pipe_redir.txt\n",
    );
    assert_eq!(out.trim(), "Hello world");
    let _ = std::fs::remove_file("/tmp/grfsh_pipe_redir.txt");
}

#[test]
fn glob_expansion() {
    let out = run("echo src/*.rs\n");
    assert!(out.contains("src/main.rs"));
    assert!(out.contains("src/shell.rs"));
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

#[test]
fn substitution_skipped_when_branch_not_taken() {
    let marker = "/tmp/grfsh_lazy_subst_marker";
    let _ = std::fs::remove_file(marker);
    run(&format!("false && echo $(touch {marker})\n"));
    assert!(!std::path::Path::new(marker).exists());
}

#[test]
fn exit_status_variable() {
    assert_eq!(run("false; echo $?\ntrue; echo $?\n").trim(), "1\n0");
}

#[test]
fn parameter_default_and_length() {
    assert_eq!(run("x=hello\necho ${y:-dflt} ${#x}\n").trim(), "dflt 5");
}

#[test]
fn arithmetic_expansion() {
    assert_eq!(run("i=5\necho $((i * 2 + 1))\n").trim(), "11");
}

#[test]
fn field_splitting_respects_quotes() {
    let out = run(
        "x='a  b'\nfor w in $x; do echo \"[$w]\"; done\nfor w in \"$x\"; do echo \"[$w]\"; done\n",
    );
    assert_eq!(out.trim(), "[a]\n[b]\n[a  b]");
}

#[test]
fn stderr_redirection() {
    assert_eq!(
        run("ls /grfsh_nonexistent 2>/dev/null || echo failed\n").trim(),
        "failed"
    );
    assert_eq!(
        run("ls /grfsh_nonexistent 2>&1 | grep -c grfsh_nonexistent\n").trim(),
        "1"
    );
}

#[test]
fn heredoc_and_herestring() {
    assert_eq!(run("v=42\ncat <<EOF\nvalue $v\nEOF\n").trim(), "value 42");
    assert_eq!(run("cat <<'EOF'\nvalue $v\nEOF\n").trim(), "value $v");
    assert_eq!(run("tr a-z A-Z <<< hello\n").trim(), "HELLO");
}

#[test]
fn if_elif_else() {
    let out = run(
        "for n in 1 2 3; do if [ $n = 1 ]; then echo one; elif [ $n = 2 ]; then echo two; else echo many; fi; done\n",
    );
    assert_eq!(out.trim(), "one\ntwo\nmany");
}

#[test]
fn while_loop_with_break() {
    let out = run("n=0\nwhile true; do n=$((n+1)); [ $n = 3 ] && break; done\necho $n\n");
    assert_eq!(out.trim(), "3");
}

#[test]
fn multiline_constructs() {
    let out = run("for i in a b\ndo\n  echo $i\ndone\n");
    assert_eq!(out.trim(), "a\nb");
}

#[test]
fn functions_args_and_return() {
    let out = run("greet() {\n  echo \"hi $1 ($#)\"\n  return 4\n}\ngreet bob x\necho $?\n");
    assert_eq!(out.trim(), "hi bob (2)\n4");
}

#[test]
fn local_variables() {
    assert_eq!(
        run("f() { local v=in; echo $v; }\nv=out\nf\necho $v\n").trim(),
        "in\nout"
    );
}

#[test]
fn subshell_does_not_leak() {
    assert_eq!(run("x=1\n(x=2; echo $x)\necho $x\n").trim(), "2\n1");
}

#[test]
fn builtin_in_pipeline() {
    assert_eq!(run("{ echo a; echo b; } | wc -l | tr -d ' '\n").trim(), "2");
}

#[test]
fn alias_after_pipe() {
    assert_eq!(run("alias up='tr a-z A-Z'\necho hi | up\n").trim(), "HI");
}

#[test]
fn prefix_assignment_is_temporary() {
    let out = run("GRFSH_T=1 env | grep GRFSH_T\necho \"[${GRFSH_T}]\"\n");
    assert_eq!(out.trim(), "GRFSH_T=1\n[]");
}

#[test]
fn exit_code_propagates() {
    let status = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"exit 7\necho never\n")
                .ok();
            child.wait()
        })
        .unwrap();
    assert_eq!(status.code(), Some(7));
}

fn run_args(args: &[&str]) -> (String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(args)
        .env("HOME", "/tmp/grfsh_test_home")
        .output()
        .expect("failed to run grfsh");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn dash_c_with_arguments() {
    let (out, status) = run_args(&["-c", "echo \"$0 $1 $#\"; exit 3", "name", "first"]);
    assert_eq!(out.trim(), "name first 1");
    assert_eq!(status, 3);
}

#[test]
fn script_file_with_arguments() {
    let path = "/tmp/grfsh_script_test.sh";
    std::fs::write(path, "for arg in \"$@\"; do\n  echo \"<$arg>\"\ndone\n").unwrap();
    let (out, _) = run_args(&[path, "a b", "c"]);
    assert_eq!(out.trim(), "<a b>\n<c>");
    let _ = std::fs::remove_file(path);
}

#[test]
fn read_splits_fields() {
    let out = run("printf 'one two three\\n' | { read a rest; echo \"$a|$rest\"; }\n");
    assert_eq!(out.trim(), "one|two three");
}

#[test]
fn while_read_loop_from_file() {
    let out = run(
        "printf 'x\\ny\\n' > /tmp/grfsh_read_loop.txt\nwhile read line; do echo \"got $line\"; done < /tmp/grfsh_read_loop.txt\n",
    );
    assert_eq!(out.trim(), "got x\ngot y");
    let _ = std::fs::remove_file("/tmp/grfsh_read_loop.txt");
}

#[test]
fn source_defines_functions() {
    std::fs::write(
        "/tmp/grfsh_source_test.sh",
        "sourced() { echo from-source $1; }\n",
    )
    .unwrap();
    let out = run(". /tmp/grfsh_source_test.sh\nsourced ok\n");
    assert_eq!(out.trim(), "from-source ok");
    let _ = std::fs::remove_file("/tmp/grfsh_source_test.sh");
}

#[test]
fn set_replaces_positional_parameters() {
    assert_eq!(run("set -- a b c\necho $# $2\n").trim(), "3 b");
}

#[test]
fn background_job_and_wait() {
    let out = run("sleep 0.2 &\njobs\nwait\necho waited $?\n");
    assert!(out.contains("Running"));
    assert!(out.contains("waited 0"));
}

#[test]
fn kill_job_by_spec() {
    assert_eq!(run("sleep 5 &\nkill %1\nwait\necho $?\n").trim(), "143");
}

#[test]
fn unknown_command_suggests_a_close_match() {
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(["-c", "ehco hi; echo status=$?"])
        .output()
        .expect("failed to run grfsh");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("did you mean 'echo'?"), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "status=127");
}
