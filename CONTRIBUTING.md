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
