use std::process::{Command, Stdio};

fn run_bytes(
    args: &[&std::ffi::OsStr],
    environment: &[(&str, &std::ffi::OsStr)],
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_grfsh"));
    command.args(args).env("HOME", "/tmp/grfsh_test_home");
    for (name, value) in environment {
        command.env(name, value);
    }
    command.output().expect("failed to run grfsh")
}

fn bytes_directory(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!("grfsh_bytes_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn os(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[test]
fn printf_and_echo_write_raw_bytes() {
    let output = run_bytes(
        &[
            os(b"-c"),
            os(b"printf '\\xc3\\xa9|\\xff|%b|' '\\303\\251'; echo -e '\\033[31m'"),
        ],
        &[],
    );
    assert_eq!(output.stdout, b"\xc3\xa9|\xff|\xc3\xa9|\x1b[31m\n");
}

#[test]
fn scripts_with_latin1_bytes_and_binary_payload_run() {
    let directory = bytes_directory("script");
    let script = directory.join("installer.sh");
    let mut contents = b"# caf\xe9 \xff\necho \"caf\xe9\"\nexit 4\n".to_vec();
    contents.extend((0..=255u8).chain(*b"'\"`$(\n"));
    std::fs::write(&script, contents).unwrap();
    let output = run_bytes(&[script.as_os_str()], &[]);
    assert_eq!(output.stdout, b"caf\xe9\n");
    assert_eq!(output.status.code(), Some(4));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let check = run_bytes(&[os(b"--check"), script.as_os_str()], &[]);
    assert!(
        !String::from_utf8_lossy(&check.stderr).contains("UTF-8"),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn script_reading_survives_low_descriptor_redirections() {
    let directory = bytes_directory("descriptors");
    let script = directory.join("fds.sh");
    std::fs::write(
        &script,
        "exec 3>/dev/null 4>/dev/null\nexec 3>&- 4>&-\necho still running\n",
    )
    .unwrap();
    let output = run_bytes(&[script.as_os_str()], &[]);
    assert_eq!(output.stdout, b"still running\n");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn non_utf8_arguments_and_environment_round_trip() {
    let output = run_bytes(
        &[
            os(b"-c"),
            os(b"printf '%s|%s|' \"$1\" \"$BYTES_TEST\"; export -p | grep BYTES_TEST; export AGAIN=\"$1\"; printenv AGAIN"),
            os(b"name"),
            os(b"a\xe9\xff"),
        ],
        &[("BYTES_TEST", os(b"v\xfe"))],
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        b"a\xe9\xff|v\xfe|declare -x BYTES_TEST=$'v\\376'\na\xe9\xff\n"
    );
}

#[test]
fn read_and_command_substitution_keep_bytes() {
    let directory = bytes_directory("read");
    let data = directory.join("latin1.txt");
    std::fs::write(&data, b"caf\xe9 \xff\xfe\n").unwrap();
    let script = format!(
        "read -r first second < '{0}'; printf '%s|%s|' \"$first\" \"$second\"; captured=$(cat '{0}'); printf '%s|' $captured; cat <<< \"$first\"",
        data.display()
    );
    let output = run_bytes(&[os(b"-c"), os(script.as_bytes())], &[]);
    assert_eq!(
        output.stdout,
        b"caf\xe9|\xff\xfe|caf\xe9|\xff\xfe|caf\xe9\n"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn invalid_utf8_file_names_glob_and_open() {
    let directory = bytes_directory("names");
    if std::fs::write(directory.join(os(b"bad\xffname")), "content\n").is_err() {
        let _ = std::fs::remove_dir_all(&directory);
        return;
    }
    let script = format!(
        "cd '{}' && for file in bad*; do printf '%s\\n' \"$file\"; cat \"$file\"; [ -f \"$file\" ] && echo file; done; echo more >> $'bad\\xffname'; cat bad?name",
        directory.display()
    );
    let output = run_bytes(&[os(b"-c"), os(script.as_bytes())], &[]);
    assert_eq!(
        output.stdout,
        b"bad\xffname\ncontent\nfile\ncontent\nmore\n"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn agent_output_keeps_bytes_and_json_stays_valid() {
    let script = os(b"printf 'a\\xff\\xc3\\xa9'");
    let plain = run_bytes(&[os(b"--agent"), os(b"-c"), script], &[]);
    assert_eq!(plain.stdout, b"a\xff\xc3\xa9");
    let json = run_bytes(&[os(b"--json"), os(b"-c"), script], &[]);
    let text = String::from_utf8(json.stdout).expect("JSON output is UTF-8");
    let expected = format!(
        "\"stdout\":\"a{}{}\"",
        char::REPLACEMENT_CHARACTER,
        char::from(0xe9u8)
    );
    assert!(text.contains(&expected), "{text}");
}

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
    let out = run("shopt -s expand_aliases\nalias ll='echo aliased'\nll\nunalias ll\n");
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
    assert_eq!(
        run("shopt -s expand_aliases\nalias up='tr a-z A-Z'\necho hi | up\n").trim(),
        "HI"
    );
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
    assert_eq!(
        run("sleep 5 &\nkill %1\nwait %1\necho $?\nwait\necho $?\n").trim(),
        "143\n0"
    );
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
fn where_regex_search_shorthands_and_load_details() {
    let directory = std::env::temp_dir().join(format!("grfsh_tables_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("a.rs"), "let year = 2024;\nlet name = x;\n").unwrap();
    std::fs::write(directory.join("b.txt"), "plain\n").unwrap();
    std::fs::write(directory.join("data.csv"), "\u{feff}a,b\n1,2,3\n").unwrap();
    std::fs::write(directory.join("rows.jsonl"), "{\"a\":1}\n{\"a\":\n").unwrap();
    let dir = directory.display();
    let (out, _, _) = run_command(&format!("files {dir} | where name =~ '\\.rs$' | get name"));
    assert_eq!(out, "a.rs\n");
    let (out, _, _) = run_command(&format!("search '\\d{{4}}' {dir}"));
    assert_eq!(out, format!("{dir}/a.rs:1:let year = 2024;\n"));
    let (_, err, status) = run_command(&format!("search '(' {dir}"));
    assert_eq!(status, 1);
    assert!(err.contains("invalid regular expression"), "{err}");
    let (out, _, _) = run_command(&format!("load {dir}/data.csv | to json"));
    assert_eq!(out, "[{\"a\":1,\"b\":2,\"column3\":3}]\n");
    let (_, err, _) = run_command(&format!("load {dir}/rows.jsonl"));
    assert!(err.contains("line 2: invalid JSON"), "{err}");
    let (_, err, status) = run_command(&format!("files {dir} | where size -gt abc"));
    assert_eq!(status, 1);
    assert!(err.contains("'abc' is not a valid size"), "{err}");
    let (_, err, _) = run_command(&format!("load {dir}/missing.csv"));
    assert!(err.contains("No such file or directory"), "{err}");
    assert!(!err.contains("os error"), "{err}");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn tables_escape_control_characters_and_reject_unknown_flags() {
    let directory = std::env::temp_dir().join(format!("grfsh_escapes_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("\u{1b}]0;pwned\u{7}evil"), "").unwrap();
    std::fs::write(directory.join("two\nlines"), "").unwrap();
    let dir = directory.display();
    let (out, _, _) = run_command(&format!("files {dir} | select name"));
    assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{out:?}");
    assert!(out.contains("\\e]0;pwned\\x07evil"), "{out}");
    assert!(out.contains("two\\nlines"), "{out}");
    let (out, _, _) = run_command(&format!("files {dir} | get -0 name"));
    assert_eq!(out, "\u{1b}]0;pwned\u{7}evil\0two\nlines\0");
    for command in ["git-status -x", "procs -x", "ports -a", "files --nope"] {
        let (_, err, status) = run_command(command);
        assert_eq!(status, 2, "{command}: {err}");
    }
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn git_status_paths_are_relative_to_the_current_directory() {
    let directory = std::env::temp_dir().join(format!("grfsh_git_rel_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("sub")).unwrap();
    let script = format!(
        "cd {dir} && git init -q . && echo a > top.txt && echo b > sub/inner.txt && cd sub && git-status | sort-by path | get path",
        dir = directory.display()
    );
    let (out, err, _) = run_command(&script);
    assert_eq!(out, "../top.txt\ninner.txt\n", "{err}");
    let _ = std::fs::remove_dir_all(&directory);
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
    assert_eq!(status, 127);
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

#[test]
fn table_grouping_math_and_compound_where() {
    let path = write_fixture(
        "hosts.csv",
        "host,region,cpu\nweb-1,eu,72\nweb-2,eu,18\ndb-1,us,91\ncache-1,us,9\n",
    );
    assert_eq!(
        run(&format!("load {path} | group-by region | to csv\n")).trim(),
        "region,count\neu,2\nus,2"
    );
    assert_eq!(run(&format!("load {path} | math avg cpu\n")).trim(), "47.5");
    assert_eq!(
        run(&format!(
            "load {path} | where region == eu and cpu -gt 50 or host == cache-1 | get host\n"
        ))
        .trim(),
        "web-1\ncache-1"
    );
    assert_eq!(
        run(&format!(
            "load {path} | update region all | uniq-by region | count\n"
        ))
        .trim(),
        "1"
    );
}

#[test]
fn login_shell_reads_profile_then_rc() {
    let home = "/tmp/grfsh_login_test_home";
    let _ = std::fs::remove_dir_all(home);
    std::fs::create_dir_all(format!("{home}/.config/grfsh")).unwrap();
    std::fs::write(format!("{home}/.grfsh_profile"), "echo profile\n").unwrap();
    std::fs::write(format!("{home}/.config/grfsh/grfshrc"), "echo rc\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .arg("-il")
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run grfsh");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "profile\nrc\n");
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .arg("-l")
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run grfsh");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "profile\n");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn associative_arrays_and_declare() {
    let (out, _, _) = run_command(
        "declare -A cfg=([host]=db-1 [port]=5432); cfg[user]=admin; for k in \"${!cfg[@]}\"; do echo \"$k=${cfg[$k]}\"; done; echo ${#cfg[@]}",
    );
    assert_eq!(out, "port=5432\nhost=db-1\nuser=admin\n3\n");
}

#[test]
fn sparse_arrays_and_unset_elements() {
    let (out, _, _) =
        run_command("a=(x y z); a[10]=k; unset 'a[1]'; echo ${#a[@]} ${!a[@]} ${a[-1]}");
    assert_eq!(out.trim(), "3 0 2 10 k");
}

#[test]
fn declaration_builtins_do_not_split_values() {
    let (out, _, _) = run_command(
        "f() { local v=$(echo one two); local -a items=(a 'b c'); echo \"[$v] ${#items[@]}\"; }; f; export E=$(echo a b); env | grep '^E='",
    );
    assert_eq!(out, "[one two] 2\nE=a b\n");
}

#[test]
fn readonly_and_integer_attributes() {
    let (out, error, _) = run_command(
        "readonly PI=3.14; PI=3; echo skipped\necho $PI; declare -i n=2+3; n+=4; echo $n; declare -l low=MiXeD; echo $low",
    );
    assert!(error.contains("PI: readonly variable"));
    assert_eq!(out, "3.14\n9\nmixed\n");
}

#[test]
fn getopts_parses_flags_and_arguments() {
    let path = write_fixture(
        "getopts.sh",
        "while getopts 'vo:n:' opt; do\n  case $opt in\n    v) echo verbose ;;\n    o) echo \"output=$OPTARG\" ;;\n    n) echo \"count=$OPTARG\" ;;\n    \\?) echo bad ;;\n  esac\ndone\nshift $((OPTIND - 1))\necho \"rest: $*\"\n",
    );
    let (out, _) = run_args(&[&path, "-v", "-o", "out.txt", "-n5", "-x", "a", "b"]);
    assert_eq!(out, "verbose\noutput=out.txt\ncount=5\nbad\nrest: a b\n");
}

#[test]
fn shopt_globstar_and_nullglob() {
    let (out, _, _) = run_command(
        "shopt -s globstar nullglob; for f in src/**/getopts.rs; do echo $f; done; echo \"(\" *.zzz \")\"",
    );
    assert_eq!(out, "src/builtins/getopts.rs\n( )\n");
}

#[test]
fn pipestatus_and_line_numbers() {
    let path = write_fixture(
        "lines.sh",
        "false | true | (exit 3)\necho \"${PIPESTATUS[@]}\"\necho $LINENO\nnope_command_x\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .arg(&path)
        .output()
        .expect("failed to run grfsh");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "1 0 3\n3\n");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("lines.sh: line 4: nope_command_x: command not found")
    );
}

#[test]
fn process_substitution_and_file_read() {
    let (out, _, _) = run_command(
        "diff <(printf 'a\\nb\\n') <(printf 'a\\nc\\n') >/dev/null; echo diff=$?; while read l; do echo got $l; done < <(printf 'x\\ny\\n'); printf 'from file' > /tmp/grfsh_subst_read.txt; echo \"[$(< /tmp/grfsh_subst_read.txt)]\"",
    );
    assert_eq!(out, "diff=1\ngot x\ngot y\n[from file]\n");
}

#[test]
fn read_flags_and_mapfile() {
    let out = run(
        "printf 'a b c\\n' | { read -a parts; echo ${#parts[@]} ${parts[2]}; }\nprintf 'abcdef' | { read -n 3 first; echo $first; }\nprintf 'one;two' | { read -d ';' word; echo $word; }\nprintf 'l1\\nl2\\nl3\\n' | { mapfile -t lines; echo ${#lines[@]} ${lines[1]}; }\n",
    );
    assert_eq!(out, "3 c\nabc\none\n3 l2\n");
}

#[test]
fn exec_redirections_and_replacement() {
    let (out, _, _) = run_command(
        "exec 3>/tmp/grfsh_exec_fd.txt; echo saved >&3; exec 3>&-; cat /tmp/grfsh_exec_fd.txt; exec echo replaced; echo never",
    );
    assert_eq!(out, "saved\nreplaced\n");
}

#[test]
fn time_wait_next_and_umask() {
    let (out, error, _) =
        run_command("time true; umask 027; umask; sleep 0.3 & sleep 0.05 & wait -n; echo next=$?");
    assert!(error.contains("real\t"));
    assert_eq!(out, "0027\nnext=0\n");
}

#[test]
fn configure_prompt_presets_write_a_theme_file() {
    let home = "/tmp/grfsh_theme_test_home";
    let _ = std::fs::remove_dir_all(home);
    std::fs::create_dir_all(home).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args([
            "-c",
            "configure-prompt --preset pure; configure-prompt --preset nope",
        ])
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .expect("failed to run grfsh");
    let theme = std::fs::read_to_string(format!("{home}/.config/grfsh/theme")).unwrap();
    assert!(theme.contains("lines = 2"));
    assert!(theme.contains("transient = true"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown preset 'nope'"));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn abbr_bindkey_and_complete_definitions() {
    let (out, error, _) = run_command(
        "abbr gco git checkout; abbr; bindkey ^G clear-screen; bindkey ^G fly; bindkey -s ^T 'echo hi'; bindkey; complete -W 'start stop' svc; complete -p",
    );
    assert!(out.contains("abbr gco 'git checkout'"));
    assert!(out.contains("bindkey '^G' clear-screen"));
    assert!(out.contains("bindkey -s '^T' 'echo hi'"));
    assert!(out.contains("complete -W 'start stop' svc"));
    assert!(error.contains("fly: unknown action"));
}

#[test]
fn hooks_run_on_directory_change() {
    let (out, _, _) = run_command("chpwd() { echo now in $PWD; }; cd /tmp; cd /tmp");
    assert!(out.trim_end().ends_with("tmp"));
    assert_eq!(out.lines().count(), 1);
}

#[test]
fn check_reports_problems_without_running() {
    let path = write_fixture(
        "check.sh",
        "touch /tmp/grfsh_check_should_not_exist\nbind -x echo\nmystery-tool --go\n",
    );
    let _ = std::fs::remove_file("/tmp/grfsh_check_should_not_exist");
    let (out, status) = run_args(&["--check", &path]);
    assert!(out.contains(":2: error: 'bind' is not supported"));
    assert!(out.contains(":3: warning: 'mystery-tool'"));
    assert_eq!(status, 1);
    assert!(!std::path::Path::new("/tmp/grfsh_check_should_not_exist").exists());
}

#[test]
fn check_follows_sourced_files_and_stops_at_exit() {
    let directory = std::env::temp_dir().join(format!("grfsh_check_dir_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("lib.sh"), "helper() { echo; }\n").unwrap();
    let script = directory.join("main.sh");
    let mut contents = b"# caf\xe9\n. \"$(dirname \"$0\")/lib.sh\"\nhelper\nexit 0\n".to_vec();
    contents.extend_from_slice(b"\x00\xffbinary payload ((\n");
    std::fs::write(&script, contents).unwrap();
    let (out, status) = run_args(&["--check", script.to_str().unwrap()]);
    assert_eq!(out, format!("{}: ok\n", script.display()));
    assert_eq!(status, 0);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn ifs_ansi_quotes_and_arithmetic_commands() {
    let (out, _, _) = run_command(
        "IFS=: read -r a b <<< 'x:y:z'; echo \"$a|$b\"; printf '[%s]' $'t\\tq'; echo; for ((i=0; i<3; i++)); do (( i == 1 )) && echo one; done",
    );
    assert_eq!(out, "x|y:z\n[t\tq]\none\n");
}

#[test]
fn startup_files_writable_by_others_are_skipped() {
    use std::os::unix::fs::PermissionsExt;
    let home = "/tmp/grfsh_trust_test_home";
    let _ = std::fs::remove_dir_all(home);
    std::fs::create_dir_all(home).unwrap();
    let rc = format!("{home}/.grfshrc");
    std::fs::write(&rc, "echo rc-ran\n").unwrap();
    std::fs::set_permissions(&rc, std::fs::Permissions::from_mode(0o666)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .arg("-i")
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run grfsh");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("rc-ran"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("writable by other users"));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn no_color_removes_escape_codes() {
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(["-c", "nope_command_for_color_test"])
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run grfsh");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("grfsh: line 1: "), "{stderr}");
    assert!(!stderr.contains('\x1b'));
}

#[test]
fn help_lists_and_describes_builtins() {
    let (out, _, status) = run_command("help getopts");
    assert!(out.starts_with("getopts spec name [args]"));
    assert_eq!(status, 0);
}

struct Startup {
    stdout: String,
    stderr: String,
    status: i32,
}

fn startup_home(name: &str) -> String {
    let home = format!("/tmp/grfsh_startup_{name}_{}", std::process::id());
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(format!("{home}/.grfsh_profile"), "echo profile\n").unwrap();
    std::fs::write(format!("{home}/.grfshrc"), "echo rc\n").unwrap();
    home
}

fn start(home: &str, args: &[&str], input: &str) -> Startup {
    start_with(home, args, input, |_| {})
}

fn start_with(
    home: &str,
    args: &[&str],
    input: &str,
    configure: impl FnOnce(&mut Command),
) -> Startup {
    use std::io::Write;
    let mut command = Command::new(env!("CARGO_BIN_EXE_grfsh"));
    command
        .args(args)
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("GRFSH_AGENT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure(&mut command);
    let mut child = command.spawn().expect("failed to run grfsh");
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    let output = child.wait_with_output().unwrap();
    Startup {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status: output.status.code().unwrap_or(-1),
    }
}

#[test]
fn login_shells_read_the_profile_with_commands_and_scripts() {
    let home = startup_home("login");
    let script = format!("{home}/script.sh");
    std::fs::write(&script, "echo script $1\n").unwrap();
    for args in [vec!["-l", "-c", "echo cmd"], vec!["-lc", "echo cmd"]] {
        assert_eq!(start(&home, &args, "").stdout, "profile\ncmd\n");
    }
    assert_eq!(
        start(&home, &["-l", &script, "x"], "").stdout,
        "profile\nscript x\n"
    );
    let dashed = start_with(&home, &["-c", "echo $0"], "", |command| {
        use std::os::unix::process::CommandExt;
        command.arg0("-grfsh");
    });
    assert_eq!(dashed.stdout, "profile\n-grfsh\n");
    assert_eq!(start(&home, &["-c", "echo cmd"], "").stdout, "cmd\n");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn rc_is_read_only_by_interactive_shells() {
    let home = startup_home("rc");
    assert_eq!(start(&home, &[], "echo piped\n").stdout, "piped\n");
    assert_eq!(
        start(&home, &["-i", "-c", "echo $-"], "").stdout,
        "rc\nhiBHc\n"
    );
    assert_eq!(start(&home, &["-ilc", "true"], "").stdout, "profile\nrc\n");
    let quiet = start(
        &home,
        &["-il", "--norc", "--noprofile", "-c", "echo quiet"],
        "",
    );
    assert_eq!(quiet.stdout, "quiet\n");
    let custom = format!("{home}/custom");
    std::fs::write(&custom, "echo custom\n").unwrap();
    assert_eq!(
        start(&home, &["-i", "--rcfile", &custom, "-c", "true"], "").stdout,
        "custom\n"
    );
    let piped = start(&home, &["-i"], "echo from-stdin $-\n");
    assert!(piped.stdout.contains("rc\n"));
    assert!(piped.stdout.contains("from-stdin hiBHs"));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn bash_style_option_clusters() {
    let home = startup_home("options");
    let traced = start(
        &home,
        &["-euxo", "pipefail", "-c", "false | true; echo no"],
        "",
    );
    assert_eq!(traced.stdout, "");
    assert_eq!(traced.status, 1);
    assert!(traced.stderr.contains("+ false"));
    let errexit = start(&home, &["-ce", "false; echo no", "name"], "");
    assert_eq!((errexit.stdout.as_str(), errexit.status), ("", 1));
    let later = start(&home, &["-c", "-e", "echo $0 $1", "zero", "one"], "");
    assert_eq!(later.stdout, "zero one\n");
    let stdin = start(&home, &["-s", "a", "b"], "echo $1 $2 $#\n");
    assert_eq!(stdin.stdout, "a b 2\n");
    let dash = start(&home, &["-", "a"], "");
    assert_eq!(dash.status, 127);
    assert!(dash.stderr.contains("a: No such file or directory"));
    let ended = start(&home, &["--", "-c"], "");
    assert!(ended.stderr.contains("-c: No such file or directory"));
    let invalid = start(&home, &["-Q"], "");
    assert_eq!(invalid.status, 2);
    assert!(invalid.stderr.contains("-Q: invalid option"));
    assert!(invalid.stderr.contains("usage:"));
    let missing = start(&home, &["-c"], "");
    assert_eq!(missing.status, 2);
    assert!(missing.stderr.contains("-c: option requires an argument"));
    let shopt = start(&home, &["-O", "nullglob", "-c", "shopt nullglob"], "");
    assert!(shopt.stdout.starts_with("nullglob"));
    assert!(shopt.stdout.trim_end().ends_with("on"));
    let posix = start(&home, &["--posix", "--norc", "-c", "echo ok"], "");
    assert_eq!(posix.stdout, "ok\n");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn no_execute_checks_syntax_only() {
    let home = startup_home("noexec");
    let good = format!("{home}/good.sh");
    let bad = format!("{home}/bad.sh");
    std::fs::write(&good, "echo ran\n").unwrap();
    std::fs::write(&bad, "echo ran\nfi\n").unwrap();
    assert_eq!(start(&home, &["-n", &good], "").stdout, "");
    let failed = start(&home, &["-n", &bad], "");
    assert_eq!((failed.stdout.as_str(), failed.status), ("", 2));
    assert!(failed.stderr.contains("line 2"));
    assert_eq!(start(&home, &["-n", "-c", "echo ran"], "").stdout, "");
    assert_eq!(start(&home, &["-n"], "echo ran\n").status, 0);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn environment_defaults_at_startup() {
    let home = startup_home("environment");
    let path = start_with(&home, &["-c", "echo $PATH; ls -d /"], "", |command| {
        command.env_remove("PATH");
    });
    assert_eq!(
        path.stdout,
        "/usr/local/bin:/usr/local/sbin:/usr/bin:/usr/sbin:/bin:/sbin\n/\n"
    );
    let level = start_with(
        &home,
        &["-c", "echo $SHLVL; env | grep ^SHLVL="],
        "",
        |command| {
            command.env("SHLVL", "4");
        },
    );
    assert_eq!(level.stdout, "5\nSHLVL=5\n");
    let tilde = start_with(
        &home,
        &["-c", "echo ~; echo ${HOME-unset}"],
        "",
        |command| {
            command.env_remove("HOME");
        },
    );
    let lines: Vec<&str> = tilde.stdout.lines().collect();
    assert!(lines[0].starts_with('/'));
    assert_eq!(lines[1], "unset");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn bash_compatible_shell_variables() {
    let home = startup_home("variables");
    let script = "echo \"$PS4|$GRFSH_VERSION|${BASH_VERSION-none}\"
[[ $UID == $(id -u) && $EUID == $(id -u) ]] && echo ids
[[ $BASHPID == $$ ]] && echo pid
(( $(echo $BASHPID) != $$ )) && echo subshell
[[ $OSTYPE && $HOSTTYPE && $MACHTYPE == $HOSTTYPE-*-$OSTYPE && $HOSTNAME ]] && echo platform
[[ ${GROUPS[0]} == $(id -g) ]] && echo groups
[[ $EPOCHSECONDS =~ ^[0-9]+$ && $EPOCHREALTIME =~ ^[0-9]+\\.[0-9]{6}$ ]] && echo epoch
(( SRANDOM >= 0 )) && echo srandom
env | grep -c -E '^(OSTYPE|PS4|GRFSH_VERSION)='";
    let output = start(&home, &["-c", script], "");
    assert_eq!(
        output.stdout,
        format!(
            "+ |{}|none\nids\npid\nsubshell\nplatform\ngroups\nepoch\nsrandom\n0\n",
            env!("CARGO_PKG_VERSION")
        ),
        "{}",
        output.stderr
    );
    let readonly = start(&home, &["-c", "UID=0"], "");
    assert!(readonly.stderr.contains("UID: readonly variable"));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn aliases_are_interactive_unless_expand_aliases_is_set() {
    let home = startup_home("aliases");
    assert_eq!(start(&home, &["-c", "alias"], "").stdout, "");
    let plain = start(&home, &["-c", "alias hi='echo alias'\nhi"], "");
    assert!(plain.stderr.contains("hi: command not found"));
    let enabled = start(
        &home,
        &["-c", "shopt -s expand_aliases\nalias hi='echo alias'\nhi"],
        "",
    );
    assert_eq!(enabled.stdout, "alias\n");
    let interactive = start(
        &home,
        &["-i", "--norc", "-c", "alias ls; shopt expand_aliases"],
        "",
    );
    assert!(interactive.stdout.contains("ls --color=auto"));
    assert!(interactive.stdout.trim_end().ends_with("on"));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn json_covers_stdin_check_and_explain() {
    let home = startup_home("json");
    let script = format!("{home}/script.sh");
    std::fs::write(&script, "echo hi\n").unwrap();
    for (args, input) in [
        (vec!["--json"], "echo piped\n"),
        (vec!["--json", "--check", script.as_str()], ""),
        (vec!["--json", "--explain", "echo explained"], ""),
    ] {
        let output = start(&home, &args, input);
        assert!(
            output.stdout.starts_with("{\"status\":0,\"stdout\":"),
            "{args:?}: {}",
            output.stdout
        );
        assert_eq!(output.stdout.lines().count(), 1);
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn unsafe_startup_directory_is_reported_once() {
    use std::os::unix::fs::PermissionsExt;
    let home = startup_home("trust_once");
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o777)).unwrap();
    let output = start(&home, &["-il", "-c", "true"], "");
    assert_eq!(output.stderr.matches("not loading").count(), 1);
    assert_eq!(output.stdout, "");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn cd_up_from_a_deleted_directory() {
    let home = startup_home("deleted");
    let script =
        format!("mkdir -p {home}/a/b; cd {home}/a/b; rmdir ../b ../../a; cd ..; echo \"$? $PWD\"");
    let output = start(&home, &["-c", &script], "");
    assert_eq!(output.stdout, format!("0 {home}/a/b/..\n"));
    assert!(
        output
            .stderr
            .contains("cd: error retrieving current directory")
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn help_points_to_the_docs_and_covers_time() {
    let (out, _, status) = run_command("help");
    assert!(out.contains("https://grf.sh/docs/"), "{out}");
    assert_eq!(status, 0);
    let (out, _, _) = run_command("help time timeout");
    assert!(out.starts_with("time [-p] pipeline"), "{out}");
    assert!(out.contains("--preserve-status"), "{out}");
}

#[test]
fn verbose_and_noexec_options_in_scripts() {
    let directory = std::env::temp_dir().join(format!("grfsh_options_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let script = directory.join("options.sh");
    std::fs::write(
        &script,
        "set -v\necho shown\nset +v\nset -n\necho skipped\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .arg(&script)
        .output()
        .expect("failed to run grfsh");
    std::fs::remove_dir_all(&directory).ok();
    assert_eq!(String::from_utf8_lossy(&output.stdout), "shown\n");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "echo shown\nset +v\n"
    );
}

fn run_script_text(name: &str, text: &str) -> (String, String, i32) {
    let path = std::env::temp_dir().join(format!("grfsh_parse_{name}_{}.sh", std::process::id()));
    std::fs::write(&path, text).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .arg(&path)
        .env("HOME", "/tmp/grfsh_test_home")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run grfsh");
    let _ = std::fs::remove_file(&path);
    assert!(output.status.code().is_some(), "{name}: killed by a signal");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn syntax_errors_stop_the_script_without_running_the_broken_command() {
    let (out, error, status) = run_script_text(
        "syntax",
        "echo one\nif [[ -o errexit ]]; then echo BODY; fi\nif then (\necho two\nfi\necho three\n",
    );
    assert_eq!(out, "one\n");
    assert!(error.contains("line 3"), "{error}");
    assert_eq!(status, 2);
    let (out, status) = run_args(&["-c", "echo one; if then (; echo two; fi; echo three"]);
    assert_eq!((out.as_str(), status), ("", 2));
    assert_eq!(run("echo a\nfi\necho b\n"), "a\n");
}

#[test]
fn scripts_keep_blank_lines_and_carriage_returns() {
    let (out, _, _) = run_script_text("blank", "cat <<E\na\n\nb\nE\nx=\"1\n\n2\"\necho \"$x\"\n");
    assert_eq!(out, "a\n\nb\n1\n\n2\n");
    let (out, _, _) = run_script_text("crlf", "echo hi\r\n");
    assert_eq!(out, "hi\r\n");
}

#[test]
fn heredoc_at_end_of_file_warns_and_keeps_its_text() {
    let (out, error, status) = run_script_text("heredoc", "cat <<EOF\na\nb\n");
    assert_eq!(out, "a\nb\n");
    assert!(
        error.contains("here-document at line 1 delimited by end-of-file (wanted `EOF')"),
        "{error}"
    );
    assert_eq!(status, 0);
}

#[test]
fn long_scripts_are_parsed_in_linear_time() {
    let body: String = (0..20_000)
        .map(|index| format!("  echo line {index}\n"))
        .collect();
    let heredoc: String = (0..20_000).map(|index| format!("row {index}\n")).collect();
    let script = format!("f() {{\n{body}}}\nf | tail -1\ncat <<EOF | tail -1\n{heredoc}EOF\n");
    let started = std::time::Instant::now();
    let (out, _, _) = run_script_text("long", &script);
    assert_eq!(out, "line 19999\nrow 19999\n");
    assert_eq!(run(&script), "line 19999\nrow 19999\n");
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}

#[test]
fn deep_nesting_is_an_error_instead_of_a_crash() {
    let braces = format!(
        "{}echo deep; {}\necho after\n",
        "{ ".repeat(10_000),
        "} ".repeat(10_000)
    );
    let (out, error, status) = run_script_text("braces", &braces);
    assert_eq!(out, "");
    assert!(error.contains("maximum nesting depth exceeded"), "{error}");
    assert_eq!(status, 2);
    let parens = format!("{}echo deep{}\n", "(".repeat(100_000), ")".repeat(100_000));
    run_script_text("parens", &parens);
    let defaults = format!("echo {}x{}\n", "${x:-".repeat(10_000), "}".repeat(10_000));
    run_script_text("defaults", &defaults);
    let arithmetic = format!(
        "echo $(({}1{}))\necho after\n",
        "(".repeat(100_000),
        ")".repeat(100_000)
    );
    let (out, _, _) = run_script_text("arithmetic", &arithmetic);
    assert!(out.ends_with("after\n"));
    let (out, error, _) = run_script_text(
        "source",
        "depth=$((depth + 1))\nif (( depth < 100000 )); then . \"$0\"; fi\necho done $depth\n",
    );
    assert!(
        error.contains("maximum nesting depth exceeded")
            || error.contains("expression recursion level exceeded"),
        "{error}"
    );
    assert!(out.contains("done"));
    let (_, error, _) = run_script_text("functions", "f() { g; }\ng() { f; }\nf\n");
    assert!(
        error.contains("maximum function nesting exceeded"),
        "{error}"
    );
}

#[test]
fn select_is_a_loop_only_with_loop_syntax() {
    let directory = std::env::temp_dir().join(format!("grfsh_select_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("a"), "").unwrap();
    let dir = directory.display();
    let (out, _, status) = run_command(&format!("files {dir} | select name"));
    assert_eq!((out.as_str(), status), ("name\na\n", 0));
    let (out, _, _) = run_command("select x in red; do echo \"picked $x\"; break; done <<< 1");
    assert_eq!(out, "picked red\n");
    let (out, _, _) = run_command("set -- p q; select x\ndo echo \"got $x\"; break; done <<< 2");
    assert_eq!(out, "got q\n");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn exported_functions_reach_child_shells() {
    let binary = env!("CARGO_BIN_EXE_grfsh");
    let (out, _, _) = run_command(&format!(
        "greet() {{ echo \"hello $1\"; }}; export -f greet; {binary} -c 'greet child'; greet() {{ echo \"again $1\"; }}; {binary} -c 'greet redefined'; export -nf greet; {binary} -c 'greet gone' 2>/dev/null || echo unexported"
    ));
    assert_eq!(out, "hello child\nagain redefined\nunexported\n");
}

#[test]
fn exported_function_environment_uses_bash_format() {
    let (out, _, _) = run_command("f() { echo f; }; export -f f; env | grep '^BASH_FUNC_f%%='");
    assert_eq!(out, "BASH_FUNC_f%%=() { echo f; }\n");
}

#[test]
fn array_append_and_length_stay_fast() {
    let started = std::time::Instant::now();
    let (out, _, _) = run_command(
        "a=(); for ((i = 0; i < 100000; i++)); do a+=(\"item $i\"); n=${#a[@]}; done; echo $n ${a[99999]}",
    );
    assert_eq!(out, "100000 item 99999\n");
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}

#[test]
fn readonly_assignment_aborts_only_its_line_in_command_strings() {
    let (out, error, status) =
        run_command("readonly R=1\nR=2; echo skipped\necho next $?\nunset R\necho unset $?");
    assert!(error.contains("R: readonly variable"));
    assert!(error.contains("unset: R: cannot unset: readonly variable"));
    assert_eq!(out, "next 1\nunset 1\n");
    assert_eq!(status, 0);
}

#[test]
fn search_fails_when_nothing_matches() {
    let path = write_fixture("search_status.txt", "alpha\nbeta\n");
    let (_, _, missing) = run_command(&format!("search zzz_absent {path} >/dev/null"));
    assert_eq!(missing, 1);
    let (_, _, found) = run_command(&format!("search beta {path} >/dev/null"));
    assert_eq!(found, 0);
}
