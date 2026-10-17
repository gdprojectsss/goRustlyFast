use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Outcome {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    signal: Option<i32>,
    elapsed: Duration,
}

fn scratch_file(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("grfsh_processes_{}_{name}", std::process::id()))
}

fn run_bounded(script: &str, limit: Duration) -> Outcome {
    let tag = format!("{:x}", script.len() ^ (script.as_ptr() as usize));
    let stdout_path = scratch_file(&format!("{tag}.out"));
    let stderr_path = scratch_file(&format!("{tag}.err"));
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_grfsh"))
        .args(["-c", script])
        .env("GRFSH", env!("CARGO_BIN_EXE_grfsh"))
        .env("HOME", "/tmp/grfsh_processes_home")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout_path).unwrap())
        .stderr(std::fs::File::create(&stderr_path).unwrap())
        .process_group(0)
        .spawn()
        .expect("failed to run grfsh");
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > limit {
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.wait();
            panic!("script did not finish within {limit:?}: {script}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let elapsed = started.elapsed();
    let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
    let _ = std::fs::remove_file(stdout_path);
    let _ = std::fs::remove_file(stderr_path);
    Outcome {
        stdout,
        stderr,
        code: status.code(),
        signal: status.signal(),
        elapsed,
    }
}

const LIMIT: Duration = Duration::from_secs(10);

#[test]
fn int_trap_exit_status_survives_a_loop() {
    let outcome = run_bounded(
        "trap 'echo trapped; exit 8' INT; (sleep 0.3; kill -INT $$) & while :; do sleep 0.05; done",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "trapped\n");
    assert_eq!(outcome.code, Some(8));
}

#[test]
fn traps_run_while_wait_blocks() {
    let outcome = run_bounded(
        "trap 'echo got term; kill $!; exit 7' TERM; (sleep 0.3; kill -TERM $$) & sleep 5 & wait; echo never",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "got term\n");
    assert_eq!(outcome.code, Some(7));
    assert!(outcome.elapsed < Duration::from_secs(3));
}

#[test]
fn wait_returns_after_a_trapped_signal() {
    let outcome = run_bounded(
        "trap 'echo usr1' USR1; sleep 2 & (sleep 0.3; kill -USR1 $$) & wait %1; echo status $?; kill %1; wait %1; echo killed $?",
        LIMIT,
    );
    assert_eq!(
        outcome.stdout,
        format!("usr1\nstatus {}\nkilled 143\n", 128 + libc::SIGUSR1)
    );
    assert!(outcome.elapsed < Duration::from_secs(2));
}

#[test]
fn exit_trap_runs_when_terminated() {
    let outcome = run_bounded("trap 'echo cleanup' EXIT; kill -TERM $$; echo after", LIMIT);
    assert_eq!(outcome.stdout, "cleanup\n");
    assert_eq!(outcome.signal, Some(libc::SIGTERM));
}

#[test]
fn exit_trap_runs_when_terminated_during_a_foreground_command() {
    let outcome = run_bounded(
        "trap 'echo cleanup' EXIT; (sleep 0.3; kill -TERM $$) & sleep 1; echo after",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "cleanup\n");
    assert_eq!(outcome.signal, Some(libc::SIGTERM));
}

#[test]
fn exit_traps_fire_in_subshells_and_substitutions() {
    let outcome = run_bounded(
        "( trap 'echo sub' EXIT; echo in ); x=$(trap 'echo cs' EXIT; echo a); echo \"[$x]\"",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "in\nsub\n[a\ncs]\n");
}

#[test]
fn sigint_waits_for_a_child_that_ignores_it() {
    let outcome = run_bounded(
        "(sleep 0.3; kill -INT $$) & sh -c 'trap \"\" INT; sleep 1; echo child done'; echo after $?",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "child done\nafter 0\n");
    assert_eq!(outcome.code, Some(0));
}

#[test]
fn sigint_to_shell_and_child_stops_the_script() {
    let pid_file = scratch_file("child.pid");
    let script = format!(
        "(sleep 0.3; kill -INT $$ $(cat '{0}')) & sh -c \"echo \\$\\$ > '{0}'; exec sleep 2\"; echo after",
        pid_file.display()
    );
    let outcome = run_bounded(&script, LIMIT);
    let _ = std::fs::remove_file(pid_file);
    assert_eq!(outcome.stdout, "");
    assert_eq!(outcome.signal, Some(libc::SIGINT));
}

#[test]
fn writing_to_a_closed_pipe_stops_promptly() {
    let outcome = run_bounded(
        "\"$GRFSH\" -c 'while :; do echo x; done' | head -1; echo ${PIPESTATUS[0]}; for ((;;)); do printf 'y\\n'; done | head -1",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "x\n141\ny\n");
}

#[test]
fn builtins_report_write_errors() {
    let outcome = run_bounded(
        "echo hi >&-; echo rc=$?; trap '' PIPE; (sleep 0.2; echo late) | true; echo done",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "rc=1\ndone\n");
    assert!(
        outcome
            .stderr
            .contains("echo: write error: Bad file descriptor"),
        "{}",
        outcome.stderr
    );
    assert!(
        outcome.stderr.contains("echo: write error: Broken pipe"),
        "{}",
        outcome.stderr
    );
}

#[test]
fn long_loops_do_not_leave_zombies() {
    let outcome = run_bounded(
        "for i in $(seq 300); do : <(true); done
         for i in $(seq 100); do read x < <(echo a); done
         for i in $(seq 100); do echo a > >(cat >/dev/null); done
         i=0; while [ $i -lt 150 ]; do true & i=$((i+1)); done
         sleep 0.3; true
         ps -A -o stat= -o ppid= | awk -v parent=$$ '$1 ~ /Z/ && $2 == parent' | wc -l | tr -d ' '",
        Duration::from_secs(60),
    );
    assert_eq!(outcome.stdout, "0\n");
}

#[test]
fn timeout_bounds_and_kill_status() {
    let outcome = run_bounded(
        "timeout 1e20 true; echo $?; timeout -s KILL 0.2 sleep 5; echo $?; timeout --preserve-status 0.2 sleep 5; echo $?",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "0\n137\n143\n");
}

#[test]
fn time_output_follows_redirections_and_timeformat() {
    let outcome = run_bounded(
        "x=$( { time true; } 2>&1 ); echo \"${x//[0-9]/N}\"; TIMEFORMAT='took %0R'; { time true; } 2>&1; time -p true 2>/dev/null",
        LIMIT,
    );
    assert_eq!(
        outcome.stdout,
        "\nreal\tNmN.NNNs\nuser\tNmN.NNNs\nsys\tNmN.NNNs\ntook 0\n"
    );
    assert!(outcome.stderr.starts_with("real 0.0"), "{}", outcome.stderr);
}

#[test]
fn exec_options_change_the_new_program() {
    let outcome = run_bounded(
        "(exec -a renamed sh -c 'echo $0'); (exec -l sh -c 'echo $0'); X=1; export X; (exec -c sh -c 'echo ${X-unset}')",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "renamed\n-sh\nunset\n");
}

#[test]
fn background_commands_ignore_sigint_in_scripts() {
    let outcome = run_bounded(
        "sleep 1 & pid=$!; sleep 0.2; kill -INT $pid; sleep 0.2; kill -0 $pid && echo still running; kill $pid",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "still running\n");
}

#[test]
fn substitutions_inherit_errexit_only_when_asked() {
    let outcome = run_bounded(
        "set -e; x=$(false; echo y); echo \"x=$x\"; shopt -s inherit_errexit; x=$(false; echo z); echo \"x=$x\"",
        LIMIT,
    );
    assert_eq!(outcome.stdout, "x=y\n");
    assert_eq!(outcome.code, Some(1));
}
