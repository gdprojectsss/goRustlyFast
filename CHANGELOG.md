# Changelog

All notable changes to grfsh are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [semantic versioning](https://semver.org). See the [stability promise](https://grf.sh/docs/stability/) for what that covers.

## [1.0.2] - 2026-10-17

### Fixed

- `grfsh --check` reports zsh only commands such as `setopt` and `autoload` as warnings, since portable scripts like the nvm and rustup installers call them behind a zsh check.

## [1.0.1] - 2026-10-17

### Fixed

- `export NAME` and `export NAME=value` emptied variables inherited from the environment, including `PATH`. Exporting a variable that was never set no longer creates an empty one.
- Login shells on macOS now add `/etc/paths` and `/etc/paths.d` to `PATH`, like `path_helper`, so Homebrew and other system wide tools are found.
- `grfsh --check` reports syntax errors on the line where they occur, explains zsh syntax such as `${(%)...}` flags, `setopt`, `autoload`, oh-my-zsh and powerlevel10k, and keeps checking the rest of the file after an error.
- Empty `if`, `elif`, `else`, `while` and `for` conditions or bodies are syntax errors, as in bash.
- `grfsh configure-prompt` and other builtins or table commands named on the command line run directly.
- The installer says how to try grfsh without changing the login shell.

## [1.0.0] - 2026-10-17

The first stable release.

### Added

- Associative and sparse arrays, `declare`, `typeset`, `readonly` and `local` flags, `getopts`, `shopt` globbing options, `$PIPESTATUS`, `$SECONDS`, `$LINENO`, `**` globs, process substitution, `time` and `exec` with file descriptors.
- Native prompt themes with right and transient prompts, the `configure-prompt` wizard, hooks, vi mode, `abbr`, `bindkey`, `complete` and plugins.
- `grfsh --check` and `history --table`.
- Agent mode (`--agent`, `GRFSH_AGENT=1`): pagers, colors and interactive prompts off, no controlling terminal, an output budget with the full text saved to a private log, and background job logs.
- `--json` results with status, stdout, stderr, duration, final directory and changed environment.
- `--serve` and `--client`: a persistent shell behind a Unix socket, with per request timeouts.
- `--explain` and the `explain` builtin show commands after expansion without running them, and flag risky ones.
- Builtins: `timeout`, `replace` (also fed by `search` through a pipe), `write-file`.
- Table commands: `search` (respects `.gitignore`), `git-status`, `ports`.
- `${!name}`, `${!prefix*}`, the `@Q`, `@E`, `@U`, `@u`, `@L` and `@a` transforms, `base#digits` and octal arithmetic, `$PPID`, and `;&` and `;;&` in `case`.
- A compatibility suite of 40 scripts compared byte for byte with bash 5.3, and a real world check against the rustup, nvm, Homebrew, Docker and oh-my-zsh installers and neofetch.
- Fuzzing targets for the parser, word expansion, patterns, arithmetic and table parsing.
- Tests that drive the interactive shell through a pseudo terminal.
- A panic handler that restores the terminal, and hangup handling that ends jobs.
- Startup files and plugins are skipped when another user can change them.
- `NO_COLOR`, 256 and 24 bit color detection, ASCII fallbacks outside UTF-8 locales, `help` for every builtin, and a first run hint.
- Release binaries for five targets including a static musl build, `.deb` and `.rpm` packages, a cosign signed `SHA256SUMS`, an install script, Arch PKGBUILDs for a binary and a source package, a Nix flake, a Homebrew formula and a man page.
- Documentation at [grf.sh/docs](https://grf.sh/docs/).

### Changed

- Error messages are only colored when standard error is a terminal.

## [0.2.0] - 2026-08-22

Never tagged. The version in Cargo.toml during phases 3 and 4.

- AST parser with expansion at run time, job control and script support.
- `case`, `[[ ]]`, arrays, parameter operators, brace expansion, `trap`, `set -euo pipefail`, `printf`, `pushd` and history expansion.
- `z`, context aware completion, "did you mean" and an asynchronous git prompt.
- Typed table pipelines with JSON and CSV, `group-by`, `uniq-by`, `math` and `update`.
- Login shell mode and XDG configuration.

## [0.1.0] - 2026-03-26

Never tagged. The first versions: commands, pipes, redirections, colors, history, globs, command substitution and syntax highlighting.

[1.0.2]: https://github.com/gdprojectsss/goRustlyFast/releases/tag/v1.0.2
[1.0.1]: https://github.com/gdprojectsss/goRustlyFast/releases/tag/v1.0.1
[1.0.0]: https://github.com/gdprojectsss/goRustlyFast/releases/tag/v1.0.0
