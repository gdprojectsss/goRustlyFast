# Contributing to grfsh

Thanks for helping. Bug reports, fixes, docs corrections and new compatibility cases are all welcome.

## Reporting bugs

Open an [issue](https://github.com/gdprojectsss/goRustlyFast/issues/new/choose) with `grfsh --version`, your operating system, and the smallest command or script that shows the problem. If bash behaves differently, include its output too. Security problems go through the [security policy](SECURITY.md).

## Building and testing

You need Rust 1.88 or newer.

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

CI runs the same commands on Linux and macOS. Some more checks:

```sh
BASH_REFERENCE=/path/to/bash5 scripts/compat-update.sh   # regenerate compatibility expectations
scripts/real-world.sh                                     # check real installers with --check
scripts/bench.sh                                          # benchmarks, needs hyperfine
cargo +nightly fuzz run parse                             # fuzzing, from the fuzz directory
scripts/differential.sh                                   # compare grfsh with bash 5 on tests/differential
scripts/spec-tests.sh --baseline scripts/spec-baseline.txt  # Oils spec tests, fail on new failures
```

The differential and spec scripts need Python 3 and a bash 5 (`/opt/homebrew/bin/bash` when it exists, otherwise `bash`; override with `BASH_REFERENCE=/path/to/bash`). Both test `target/release/grfsh` unless you pass `--grfsh PATH`, so run `cargo build --release` first.

`scripts/differential.sh` runs every case in `tests/differential/*.sh` (cases separated by `#---` lines, each run with `-c` in an empty directory) and every script in `tests/differential/scripts` under both shells, and compares stdout, exit status and whether anything was written to stderr. Differences we keep on purpose are listed in `tests/differential/allowlist.txt`. Differences still to fix are in `tests/differential/known-failures.txt`; the script fails only on a difference outside both lists and tells you which known failures now match bash. After fixing some, run `scripts/differential.sh --write-baseline`. Use `--only syntax:` to run part of the corpus.

`scripts/spec-tests.sh` downloads a pinned commit of the [Oils](https://github.com/oils-for-unix/oils) spec tests (Apache License 2.0) into `target/oils-spec`, runs every suite that Oils compares against bash with both bash and grfsh, and prints the cases each shell passes per suite, followed by the failing grfsh cases as `FAIL<tab>suite/index<tab>description`. A grfsh case passes when it matches the expectation Oils records for bash, the ideal expectation, or what the local bash prints. `--suites arith,array` picks suites, `--details FILE` writes the code, expectations and both shells' output of each failure as JSON lines, and `--baseline scripts/spec-baseline.txt` fails when a case outside that list fails. Regenerate the list with `--write-baseline scripts/spec-baseline.txt` after fixing cases, so the number of known failures only goes down.

The `run` fuzz target executes arbitrary bytes as a script inside the shell process. It enables a sandbox (`src/sandbox.rs`) that refuses external commands and every builtin that is not on its allow list, only lets redirections write inside a temporary directory, lets each input fork at most 16 times, kills its children afterwards, and interrupts any input that runs longer than a second. Build it without a sanitizer so the stack depth matches the real shell:

```sh
cd fuzz
cargo +nightly fuzz run -s none run -- -timeout=10 -rss_limit_mb=2048 -max_len=2048 -close_fd_mask=3
```

## Compatibility cases

`tests/compat` holds scripts whose output must match bash 5 byte for byte. To add one, write `tests/compat/name.sh`, run `scripts/compat-update.sh` with a bash 5 to create `name.expected`, and make `cargo test --test compat` pass. A failing case for a bash feature is a fine contribution on its own.

## Code style

- Keep functions small and modules focused. [ARCHITECTURE.md](ARCHITECTURE.md) explains where things live.
- Use full words for names: `index`, `command`, `status`, not `idx`, `cmd`, `st`.
- Code explains itself through names and structure, so the codebase has no comments.
- New dependencies need a strong reason. grfsh depends on `rustyline` and `libc` only.
- Every user facing change gets a test, and builtins get an entry in `src/builtins/help.rs`.

## Pull requests

Keep each pull request to one change, describe what it does and why, and update [CHANGELOG.md](CHANGELOG.md) under an Unreleased heading. Docs live in the [grf-docs](https://github.com/gdprojectsss/grf-docs) repository.

## License

By contributing you agree that your contributions are licensed under the [MIT license](LICENSE).
