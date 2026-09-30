use std::path::Path;
use std::process::Command;

#[test]
fn scripts_match_bash_output() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compat");
    let mut cases: Vec<_> = std::fs::read_dir(&directory)
        .expect("tests/compat exists")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sh"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty());

    let mut failures = Vec::new();
    for case in &cases {
        let expected = std::fs::read_to_string(case.with_extension("expected"))
            .unwrap_or_else(|_| panic!("missing expected output for {}", case.display()));
        let output = Command::new(env!("CARGO_BIN_EXE_grfsh"))
            .arg(case)
            .current_dir(&directory)
            .env("HOME", "/tmp/grfsh_compat_home")
            .output()
            .expect("failed to run grfsh");
        let actual = format!(
            "{}--- exit {}\n",
            String::from_utf8_lossy(&output.stdout),
            output.status.code().unwrap_or(-1)
        );
        if actual != expected {
            failures.push(format!(
                "{}\n--- expected (bash)\n{expected}--- actual (grfsh)\n{actual}",
                case.display()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scripts differ from bash:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
