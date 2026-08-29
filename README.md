<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/logo-dark.svg">
  <img alt="grfsh" src=".github/assets/logo-light.svg" width="340">
</picture>

### A small Unix shell written in Rust.<br>Bash style scripting, job control, and pipelines that can carry typed tables.

<p>
  <a href="https://github.com/gdprojectsss/goRustlyFast/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/gdprojectsss/goRustlyFast/ci.yml?branch=main&style=flat-square&label=ci"></a>
  <a href="https://www.rust-lang.org"><img alt="Rust 2024 edition" src="https://img.shields.io/badge/rust-2024_edition-f97316?style=flat-square&logo=rust&logoColor=white"></a>
  <a href="Cargo.toml"><img alt="Dependencies: 2" src="https://img.shields.io/badge/dependencies-2-22c55e?style=flat-square"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-6366f1?style=flat-square"></a>
  <a href="https://github.com/gdprojectsss/goRustlyFast/stargazers"><img alt="GitHub stars" src="https://img.shields.io/github/stars/gdprojectsss/goRustlyFast?style=flat-square&color=22d3ee"></a>
  <img alt="Status: experimental" src="https://img.shields.io/badge/status-experimental-f59e0b?style=flat-square">
</p>

<p>
  <a href="#quick-start"><b>Quick start</b></a> |
  <a href="#table-pipelines"><b>Table pipelines</b></a> |
  <a href="#why-grfsh"><b>Why grfsh</b></a> |
  <a href="#performance"><b>Performance</b></a> |
  <a href="#how-it-works"><b>How it works</b></a> |
  <a href="#roadmap"><b>Roadmap</b></a> |
  <a href="README.ro.md"><b>Romana</b></a>
</p>

<img alt="grfsh terminal session" src=".github/assets/demo.svg" width="880">

</div>

<br>

> [!NOTE]
> grfsh is a young shell. It runs real scripts and makes a pleasant interactive shell, but it does not cover all of bash yet: `getopts`, associative arrays and process substitution are still missing. Keep bash around for system scripts.

## What is grfsh?

grfsh is a shell with bash compatible syntax for the common cases: pipes, redirections, `if`/`for`/`while`, functions, `$(...)`, `${x:-default}`, `$((math))`, heredocs and job control.

For interactive use it has autosuggestions from history, syntax highlighting, completion for git branches, ssh hosts and make targets, suggestions for mistyped commands, a `z` command for directories you visit often, and a prompt that shows the exit status and duration of the last command.

It also has a set of table commands. `files`, `procs` and `load` produce typed tables, so `where size -gt 10mb` compares sizes and `where modified gt 2d` compares dates. When a table is piped into `grep` or `wc`, it is printed as plain text.

It depends on two crates: `rustyline` for line editing and `libc` for processes and signals.

## Quick start

```sh
cargo install --git https://github.com/gdprojectsss/goRustlyFast
grfsh
```

Or build from a clone with `cargo build --release` and run `target/release/grfsh`. A Homebrew formula lives in [`packaging/homebrew`](packaging/homebrew/grfsh.rb): `brew install --HEAD ./packaging/homebrew/grfsh.rb`.

```
grfsh                          interactive shell
grfsh -l                       login shell, reads ~/.grfsh_profile first
grfsh script.sh [args...]      run a script
grfsh -c 'command' [name args] run one command line
```

To make it your login shell, add its path to `/etc/shells` and run `chsh -s "$(command -v grfsh)"`.

A few things to try once it is running:

```sh
for f in src/*/mod.rs; do echo "$f: $(wc -l < $f) lines"; done
greet() { local who=${1:-world}; echo "hi $who"; }; greet
sleep 30 &                     # then: jobs, fg, Ctrl-Z, bg
gti status                     # did you mean 'git'?
z proj                         # jump to a directory you visit often
git checkout <TAB>             # branches
```

## Table pipelines

Table commands hand each other typed rows. On a terminal the result is an aligned table, anywhere else it is plain text that `grep`, `awk` and friends understand.

```console
$ load servers.csv | where cpu -gt 50 | sort-by cpu -r
host   region  cpu  uptime_days
-----  ------  ---  -----------
db-1   us       91          203
web-1  eu       72           41

$ load servers.csv | where region == eu | to json
[{"host":"web-1","region":"eu","cpu":72,"uptime_days":41},{"host":"web-2","region":"eu","cpu":18,"uptime_days":41}]

$ ps aux | where %CPU gt 5 | select PID %CPU COMMAND
$ files src -r | where name =~ .rs | math sum size
$ load servers.csv | group-by region
```

| Command | What it does |
| --- | --- |
| `files [path] [-a] [-r]` | directory listing with name, type, size, modified, `-r` for recursive |
| `procs` | running processes with pid, ppid, cpu, mem, rss, name |
| `load <file>` | reads `.csv`, `.tsv`, `.json`, `.jsonl`, anything else as lines |
| `from csv\|tsv\|json\|jsonl\|ws\|lines` | parses text coming from a pipe |
| `where <column> <op> <value> [and\|or ...]` | keeps matching rows, ops: `== != =~ !~ eq ne lt le gt ge` |
| `sort-by <column>... [-r]` | sorts by one or more columns |
| `select` / `reject <column>...` | keeps or drops columns |
| `first [n]`, `skip [n]`, `reverse` | slices rows |
| `count`, `get <column>` | prints the row count or one column as plain lines |
| `group-by <column>` | counts rows per value |
| `uniq-by <column>` | keeps the first row for each value |
| `math sum\|avg\|min\|max\|median <column>` | aggregates a column, keeping sizes as sizes |
| `update <column> <value>` | sets or adds a column |
| `to csv\|tsv\|json\|md\|html\|table` | converts the table to text |

<details>
<summary><b>Details: values, operators and text input</b></summary>

<br>

- Values follow the column type: `10mb` or `1.5 GB` for sizes, `2d`, `3h` or `2026-01-31` for dates, plain numbers for numbers.
- `<` and `>` are redirections in every shell, so write `where size -gt 1mb` or quote the condition: `where 'size > 1mb'`.
- Output from regular commands is parsed as JSON when it looks like JSON, otherwise as whitespace separated columns with a header line, which is what `ps`, `df` and `docker ps` print.
- To run a system command that shares a name with a table command, use `command`, for example `command procs`.

</details>

## Why grfsh?

How grfsh compares to other shells on the features it focuses on:

|                                              | **grfsh** | bash | zsh | fish | nushell |
| -------------------------------------------- | :-------: | :--: | :-: | :--: | :-----: |
| POSIX style syntax (`if`, `for`, `$(...)`)   |    yes    | yes  | yes |  no  |   no    |
| Plain text pipes work unchanged              |    yes    | yes  | yes | yes  |   yes   |
| Typed table pipelines                        |    yes    |  no  | no  |  no  |   yes   |
| Autosuggestions out of the box               |    yes    |  no  | no  | yes  |   yes   |
| Syntax highlighting out of the box           |    yes    |  no  | no  | yes  |   yes   |
| Frecent directory jumping built in           |    yes    |  no  | no  |  no  |   no    |
| Mature and battle tested                     |    no     | yes  | yes | yes  | partial |

<sub>Compared with default installs, no plugins or extra configuration, as of October 2026. Corrections are welcome in an issue.</sub>

## Performance

grfsh is measured against dash, bash and zsh with [hyperfine](https://github.com/sharkdp/hyperfine). Run `scripts/bench.sh` to reproduce. Mean wall time on an Apple M2 with macOS 26.6, using the system bash 3.2.57, dash and zsh 5.9:

| Benchmark | grfsh | dash | bash | zsh |
| --- | ---: | ---: | ---: | ---: |
| Startup, `-c true` | 1.9 ms | 1.2 ms | 1.6 ms | 2.5 ms |
| 20,000 iteration `while [ ]` loop | 34.5 ms | 36.7 ms | 65.7 ms | 34.7 ms |
| Building a string 5,000 times | 17.3 ms | 49.8 ms | 911.8 ms | 106.2 ms |
| Five stage pipeline | 4.4 ms | 3.7 ms | 4.2 ms | 5.3 ms |
| 200 command substitutions | 84.5 ms | 73.2 ms | 83.0 ms | 90.5 ms |

`test` and `[` are builtins, so conditions inside loops do not fork. The bash shipped with macOS is old; a current bash will do better on the string benchmark.

## How it works

```mermaid
flowchart LR
    input["line or script"] --> lexer["lexer"]
    lexer --> parser["parser"]
    parser --> ast[("AST<br/>words keep $VAR and $(...)")]
    ast --> exec["executor"]
    exec <--> expand["expansion at run time<br/>params, $(...), math, globs"]
    exec --> builtins["builtins<br/>in-process"]
    exec --> tables["table commands"]
    exec --> processes["fork and exec<br/>process groups, pipes"]
    tables -- "typed rows" --> tables
    tables -- "aligned text" --> processes
```

- The parser keeps `$VAR`, `$(...)` and globs unexpanded in the syntax tree. They are expanded right before a command runs, so `false && echo $(rm -rf x)` does not run `rm`.
- Pipeline stages, subshells and `$(...)` run as forked children in their own process groups. The shell hands the terminal to the foreground group, so Ctrl-Z, `fg`, `bg` and Ctrl-C work.
- Two table commands next to each other exchange rows in a small typed format. Any other command receives plain text.
- Redirections are applied to file descriptors, so `2>&1`, `&>`, `>&-` and heredocs also work for builtins and functions.

## Features

<table>
<tr>
<td width="50%" valign="top">

**Scripting**
- `if`, `while`, `until`, `for`, `case`, `[[ ]]`, `test`, functions
- `${x#pat}`, `${x%pat}`, `${x/a/b}`, `${x^^}`, `${x:1:3}`, `${x:-y}`
- Indexed arrays, brace expansion `{a,b}` and `{1..10}`, `$((math))`
- `trap`, `set -euo pipefail`, `set -x`, `printf`, `read`, `source`

</td>
<td width="50%" valign="top">

**Interactive**
- Autosuggestions, syntax highlighting, multiline input
- Completion for git, cargo, ssh, make, `$VARS`, `%jobs`
- `z` for frecent directories, typing a directory enters it
- "did you mean" for mistyped commands, `!!` and `!$` history expansion

</td>
</tr>
<tr>
<td valign="top">

**Jobs and processes**
- `cmd &`, `jobs`, `fg`, `bg`, `wait`, `kill %1`
- Ctrl-Z stops the foreground job, Ctrl-C stops loops too
- Redirections on any fd: `2>`, `2>&1`, `&>`, `>&-`
- Prompt shows git branch with a dirty marker, exit status, duration

</td>
<td valign="top">

**Data**
- `files`, `procs`, `load`, `from` produce typed tables
- `where`, `sort-by`, `select`, `group-by`, `uniq-by`, `math`
- Sizes and dates compare as sizes and dates
- `to csv`, `to json`, `to md`, or plain text for other tools

</td>
</tr>
</table>

## Configuration

`~/.grfshrc`, or `~/.config/grfsh/grfshrc`, runs on startup. Login shells read `~/.grfsh_profile` first.

```sh
export EDITOR=nvim
alias ll='ls -la'
alias gs='git status'

mkcd() { mkdir -p "$1" && cd "$1"; }
```

Set `GRFSH_PROMPT` to customize the prompt. Placeholders: `{user}`, `{host}`, `{cwd}`, `{branch}`, `{dirty}`, `{git}`, `{status}`, `{duration}`, `{jobs}` and the colors `{green}`, `{blue}`, `{magenta}`, `{cyan}`, `{red}`, `{yellow}`, `{bold}`, `{reset}`.

```sh
export GRFSH_PROMPT='{blue}{cwd}{reset} {git}{red}{status}{reset}> '
```

<details>
<summary><b>Builtins</b></summary>

<br>

`cd`, `z`, `pushd`, `popd`, `dirs`, `pwd`, `printf`, `test`, `[`, `trap`, `history`, `echo`, `exit`, `export`, `unset`, `local`, `set`, `shift`, `read`, `source`, `.`, `eval`, `alias`, `unalias`, `type`, `command`, `return`, `break`, `continue`, `jobs`, `fg`, `bg`, `wait`, `kill`, `true`, `false`, `:`

</details>

<details>
<summary><b>Project layout</b></summary>

<br>

```
src/
  syntax/    lexer, parser and AST
  expand/    word expansion, arithmetic, globbing
  exec/      running commands, pipelines, redirections, processes
  builtins/  builtin commands
  table/     typed values, table commands, JSON and CSV, rendering
  repl/      prompt, completion, highlighting, the interactive loop
```

</details>

## Roadmap

| Phase | Scope | Status |
| ----- | ----- | :----: |
| 1 | Commands, pipes, redirections, colors, history | done |
| 2 | Globs, command substitution, syntax highlighting, tests, CI | done |
| 3 | AST parser, run time expansion, scripting, job control | done |
| 3 | `z`, context aware completion, "did you mean", richer prompt | done |
| 3 | Typed table pipelines, JSON and CSV | done |
| 4 | `case`, `[[ ]]`, arrays, parameter operators, brace expansion | done |
| 4 | `trap`, `set -euo pipefail`, `printf`, `pushd`, history expansion | done |
| 4 | Async git prompt, benchmarks, loop and string speedups, fuzz style tests | done |
| 4 | `group-by`, `uniq-by`, `math`, `update`, login shell, release packaging | done |
| 5 | `getopts`, `declare`, associative arrays, `$PIPESTATUS`, `**` globs, vi mode | planned |
| 5 | Prompt hooks, right prompt, starship support, oh-my-zsh theme compatibility | planned |
| 5 | `grfsh --check` for scripts, queryable history table, plugins | planned |

## Star history

<a href="https://star-history.com/#gdprojectsss/goRustlyFast&Date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=gdprojectsss/goRustlyFast&type=Date&theme=dark">
    <img alt="Star history chart" src="https://api.star-history.com/svg?repos=gdprojectsss/goRustlyFast&type=Date" width="600">
  </picture>
</a>

## Contributing

Issues and pull requests are welcome. Before sending a change, run the same checks as CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

<a href="https://github.com/gdprojectsss/goRustlyFast/graphs/contributors">
  <img alt="Contributors" src="https://contrib.rocks/image?repo=gdprojectsss/goRustlyFast">
</a>

## License

[MIT](LICENSE)
