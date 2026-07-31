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

#[test]
fn z_jumps_to_the_most_frecent_match() {
    let home = "/tmp/grfsh_z_test_home";
    let target = "/tmp/grfsh_z_test_dirs/workspace/frecent_target";
    let other = "/tmp/grfsh_z_test_dirs/workspace/rare_target";
    std::fs::create_dir_all(home).unwrap();
    std::fs::create_dir_all(target).unwrap();
    std::fs::create_dir_all(other).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(
        format!("{home}/.grfsh_dirs"),
        format!("9\t{now}\t{target}\n1\t{now}\t{other}\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(["-c", "z frecent && pwd; z rare && pwd"])
        .env("HOME", home)
        .output()
        .expect("failed to run grfsh");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("{target}\n{other}")
    );
}

#[test]
fn pipeline_writers_die_quietly_on_closed_pipe() {
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(["-c", "yes | head -1"])
        .output()
        .expect("failed to run grfsh");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "y");
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_fixture(name: &str, contents: &str) -> String {
    let path = format!("/tmp/grfsh_fixture_{name}");
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn csv_filter_sort_and_convert() {
    let path = write_fixture(
        "langs.csv",
        "name,lang,stars\ngrfsh,rust,12\nnu,rust,30000\nbash,c,999\n",
    );
    let out = run(&format!(
        "load {path} | where lang == rust | sort-by stars -r | get name\n"
    ));
    assert_eq!(out.trim(), "nu\ngrfsh");
    let json = run(&format!("load {path} | where stars -lt 1000 | to json\n"));
    assert_eq!(
        json.trim(),
        r#"[{"name":"grfsh","lang":"rust","stars":12},{"name":"bash","lang":"c","stars":999}]"#
    );
}

#[test]
fn files_where_size_uses_units() {
    let directory = "/tmp/grfsh_fixture_sizes";
    let _ = std::fs::remove_dir_all(directory);
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(format!("{directory}/small.txt"), vec![b'x'; 100]).unwrap();
    std::fs::write(format!("{directory}/large.bin"), vec![b'x'; 5000]).unwrap();
    let out = run(&format!(
        "files {directory} | where 'size > 1kb' | get name\n"
    ));
    assert_eq!(out.trim(), "large.bin");
}

#[test]
fn external_text_is_parsed_into_columns() {
    let out = run("printf 'NAME AGE\\nann 31\\nbob 27\\n' | where AGE -gt 30 | get NAME\n");
    assert_eq!(out.trim(), "ann");
}

#[test]
fn tables_print_as_aligned_text_when_piped() {
    let out = run("printf '[{\"a\":1,\"b\":\"x\"}]' | from json | first 1 | cat\n");
    assert_eq!(out.trim_end(), "a  b\n1  x");
}

#[test]
fn table_errors_name_the_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(["-c", "echo 'x y' | where nope == 1"])
        .output()
        .expect("failed to run grfsh");
    assert!(String::from_utf8_lossy(&output.stderr).contains("where: no column 'nope'"));
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn case_statement() {
    let out = run(
        "for f in a.rs b.txt c; do\n  case $f in\n    *.rs) echo rust ;;\n    *.txt|*.md) echo text ;;\n    *) echo other ;;\n  esac\ndone\n",
    );
    assert_eq!(out.trim(), "rust\ntext\nother");
}

#[test]
fn parameter_pattern_operators() {
    let out =
        run("f=src/expand/glob.rs\necho ${f##*/} ${f%.rs} ${f%%/*} ${f/glob/brace} ${f//s/S}\n");
    assert_eq!(
        out.trim(),
        "glob.rs src/expand/glob src src/expand/brace.rs Src/expand/glob.rS"
    );
}

#[test]
fn case_conversion_and_substrings() {
    assert_eq!(
        run("w=hello\necho ${w^} ${w^^} ${w:1:3} ${w: -2}\n").trim(),
        "Hello HELLO ell lo"
    );
}

#[test]
fn indexed_arrays() {
    let out = run(
        "list=(one \"two words\" three)\nlist+=(four)\necho ${#list[@]} ${list[1]} ${list[-1]}\nfor item in \"${list[@]}\"; do echo \"<$item>\"; done\n",
    );
    assert_eq!(
        out.trim(),
        "4 two words four\n<one>\n<two words>\n<three>\n<four>"
    );
}

#[test]
fn brace_expansion() {
    assert_eq!(
        run("echo {a,b}{1,2} x{,.bak} {01..03} '{a,b}'\n").trim(),
        "a1 a2 b1 b2 x x.bak 01 02 03 {a,b}"
    );
}

#[test]
fn test_builtin_and_brackets() {
    let out = run(
        "[ -d src ] && echo dir\ntest 3 -gt 5 || echo small\n[ a = b ]; echo $?\n[ 1 -eq x ]; echo $?\n",
    );
    assert_eq!(out.trim(), "dir\nsmall\n1\n2");
}

#[test]
fn double_bracket_conditions() {
    let out = run(
        "f='my file.rs'\n[[ $f == *.rs ]] && echo glob\n[[ $f == \"*.rs\" ]] || echo literal\nv=v2.15.3\n[[ $v =~ ^v([0-9]+)\\.([0-9]+) ]] && echo ${BASH_REMATCH[1]}.${BASH_REMATCH[2]}\n[[ -d src && ! -f src ]] && echo logic\n",
    );
    assert_eq!(out.trim(), "glob\nliteral\n2.15\nlogic");
}

fn run_command(script: &str) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(["-c", script])
        .output()
        .expect("failed to run grfsh");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn exit_trap_runs_on_exit() {
    let (out, _, status) = run_command("trap 'echo cleanup' EXIT; echo work; exit 3");
    assert_eq!(out.trim(), "work\ncleanup");
    assert_eq!(status, 3);
}

#[test]
fn errexit_skips_conditions_and_stops_on_failure() {
    let (out, _, status) =
        run_command("set -e; if false; then :; fi; false || true; echo alive; false; echo never");
    assert_eq!(out.trim(), "alive");
    assert_eq!(status, 1);
}

#[test]
fn nounset_reports_and_exits() {
    let (out, error, status) = run_command("set -u; echo ${fine:-ok}; echo $missing; echo never");
    assert_eq!(out.trim(), "ok");
    assert!(error.contains("missing: unbound variable"));
    assert_eq!(status, 1);
}

#[test]
fn xtrace_and_pipefail() {
    let (out, error, _) = run_command(
        "set -x; echo traced; set +x; false | true; echo $?; set -o pipefail; false | true; echo $?",
    );
    assert!(error.contains("+ echo traced"));
    assert_eq!(out.trim(), "traced\n0\n1");
}

#[test]
fn err_and_signal_traps() {
    let (out, _, _) = run_command(
        "trap 'echo err $?' ERR; false; trap 'echo usr1' USR1; kill -USR1 $$; echo done",
    );
    assert_eq!(out.trim(), "err 1\nusr1\ndone");
}

#[test]
fn printf_builtin() {
    let (out, _, _) = run_command(
        "printf '%-6s|%5.1f|%03d\\n' name 3.14159 7 x 2 42; printf -v saved '%s-%s' a b; echo $saved",
    );
    assert_eq!(out, "name  |  3.1|007\nx     |  2.0|042\na-b\n");
}

#[test]
fn directory_stack() {
    let (out, _, _) = run_command(
        "cd /tmp; pushd / >/dev/null; pushd /usr >/dev/null; dirs -v; popd >/dev/null; pwd",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[0].ends_with("/usr"));
    assert!(lines[2].ends_with("tmp"));
    assert_eq!(lines[3], "/");
}
