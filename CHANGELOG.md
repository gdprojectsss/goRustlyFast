# Changelog

All notable changes to grfsh are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [semantic versioning](https://semver.org). See the [stability promise](https://grf.sh/docs/stability/) for what that covers.

## [1.1.0] - 2026-10-17

A large compatibility and reliability release. grfsh now passes 2253 of the 2754 Oils bash spec cases (82%, bash 5.3 passes 89%), up from 1652 (60%) in 1.0.2.

### Added

- `select` menus, `coproc`, extended globs (`@( )`, `?( )`, `*( )`, `+( )`, `!( )`), name references (`declare -n`, `local -n`), `|&`, named file descriptors (`exec {fd}>file`), `$"..."` strings and `[[ -o option ]]`.
- Builtins `let`, `builtin`, `caller`, `disown`, `times`, `enable`, `compgen` and `fc`, and options for existing ones: `type -t/-p/-P/-a/-f`, `command -V/-p`, `cd -P/-L`, `pwd -P/-L`, `jobs -p/-l/-r/-s`, `wait -n/-f/-p`, `trap -p`, `mapfile -d/-O/-C`, `read -N/-u/-i`, `printf %(fmt)T` and `printf -v` into array elements, symbolic `umask`.
- Shell options `set -a`, `set -f`, `set -C` with `>|`, `set -E`, `set -T`, `set -v`, `set -n`, and `shopt` with all bash option names, including `nocasematch`, `failglob`, `lastpipe`, `inherit_errexit` and `expand_aliases`.
- `trap ... DEBUG` and `trap ... RETURN`, `export -f`, `export -n`, and the variables `UID`, `EUID`, `HOSTNAME`, `OSTYPE`, `HOSTTYPE`, `MACHTYPE`, `BASHPID`, `EPOCHSECONDS`, `EPOCHREALTIME`, `SRANDOM`, `GROUPS`, `FUNCNAME`, `BASH_SOURCE`, `BASH_LINENO`, `SHLVL`, `COLUMNS`, `LINES` and `GRFSH_VERSION`. `BASH_VERSION` stays unset on purpose, since grfsh is not bash.
- bash style command line options: clusters such as `-lc`, `-ilc` and `-euxo pipefail`, `-o`/`+o`, `-O`/`+O`, `-s`, `-n`, `--norc`, `--noprofile`, `--rcfile` and `--`.
- History expansion with `^old^new`, word designators and modifiers; `HISTSIZE`, `HISTFILESIZE` and `HISTFILE`; Alt-. for the last argument; Ctrl-E, End and Alt-F accept autosuggestions.
- `get -0`, a `-f` flag for `write-file`, real regular expressions for `where =~`, and `--check` that follows sourced files.
- Spec, differential and fuzzing test suites that run in CI.

### Fixed

- Login shells started with `-l -c`, `-lc` or `-ilc`, as editors and GUI apps do to read the environment, now read `~/.grfsh_profile` and `/etc/paths`. A default `PATH` is used when `PATH` is unset.
- History was capped at 100 entries. It now keeps 50000, writes each command as it runs and merges history from several open shells.
- Bytes that are not UTF-8 are kept intact everywhere: `printf '\xff'`, `echo -e '\033[31m'`, `read`, command substitution, file names, arguments and the environment. Scripts with Latin-1 text or a binary payload run, and non-UTF-8 arguments no longer crash grfsh.
- A syntax error no longer runs the body of the broken command, blank lines inside heredocs are kept, and long scripts parse in linear time.
- `a=1 b=$a` assigns left to right, `export NAME` before a value exports it later, `IFS` defaults to space, tab and newline, `"$*"` joins with the first character of `IFS`, and `"$@"` in an assignment keeps all arguments.
- `wait $pid`, interrupting `wait` with Ctrl-C, traps during `wait`, EXIT traps in subshells and on signals, and `set -e` no longer leaking into `$( )`.
- Writing to a closed pipe stops at once instead of spinning, background processes are reaped inside loops instead of piling up, and file descriptors from `exec 3>file` reach other programs.
- "command not found" honours `2>/dev/null`, executable files without a shebang run as scripts, and `source` searches `PATH`.
- Ctrl-C no longer swallows the next command, the git prompt never delays commands, Ctrl-D warns about stopped jobs, Tab lists candidates, and the terminal is restored after a program dies in raw mode.
- `a+=(x)`, `${#a[@]}` and `${x//pattern/replacement}` are fast on large data, and deep nesting reports an error instead of crashing.
- `where =~` matched substrings instead of regular expressions, `write-file` with no input emptied the file, `--serve` commands could read the server's input, `git-status` paths were relative to the repository root, and table output passed terminal escape codes through.
- The `ls` and `grep` color aliases apply only in interactive shells, and symlinked startup files are checked where they point.

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
