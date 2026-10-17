# Architecture

grfsh is a library crate with a thin `main.rs`. This page describes how a line of input becomes running processes and where each part of the shell lives.

## From input to processes

```
input ──► syntax::lexer ──► syntax::parser ──► AST ──► exec ──► builtins, tables, fork and exec
                                                         ▲
                                                         └── expand (at run time)
```

1. **Lexing** (`src/syntax/lexer.rs`) turns text into tokens. Words keep their structure: literal and quoted parts, parameters, command substitutions and arithmetic stay separate `WordPart`s.
2. **Parsing** (`src/syntax/parser.rs`) builds the tree in `src/syntax/ast.rs`: lists of and-or chains, pipelines, and commands such as `If`, `Loop`, `For`, `Case` and `FunctionDefinition`.
3. **Execution** (`src/exec/`) walks the tree. `mod.rs` handles control flow, `process.rs` forks pipelines, subshells and substitutions and waits for them, and `redirect.rs` applies redirections to file descriptors, saving and restoring them for builtins.
4. **Expansion** (`src/expand/`) happens right before a command runs: parameters, command substitution, arithmetic (`arith.rs`), brace expansion (`brace.rs`), word splitting and globbing (`glob.rs`). Because the tree keeps words unexpanded, `false && echo $(rm x)` never runs `rm`.

## State

`src/shell.rs` holds the `Shell` struct: variables, arrays and attributes (`src/variables.rs`), functions, aliases, options, traps, the job table (`src/jobs.rs`), the directory stack and history used by `z` (`src/directories.rs`).

## Builtins and tables

- `src/builtins/` has one file per area: control flow, directories, variables, jobs, output, `printf`, `getopts`, `test`, `trap`, files (`replace`, `write-file`) and `timeout`. `mod.rs` dispatches by name and `help.rs` documents every builtin.
- `src/table/` implements typed tables. Sources (`files`, `procs`, `search`, `git-status`, `ports`, `load`, `from`) produce a `Table` of `Value`s. Filters and aggregates transform it. Between two table commands, rows travel in a small typed format (`wire.rs`); toward any other program they are rendered as text (`render.rs`).

## Interactive shell

`src/repl/` builds on rustyline: startup files (`mod.rs`), the read loop with history saving and signal handling at the prompt (`session.rs`), completion (`completion/`), highlighting, autosuggestions, key bindings, history expansion and `fc` (`history.rs`), the asynchronous git status used by the prompt (`git.rs`), and the theme engine (`theme/`) with its segments, renderer and the `configure-prompt` wizard.

## Agent features

`src/agent/` holds agent mode: output capture and the JSON result (`mod.rs`), the output budget (`budget.rs`), `--explain` (`explain.rs`) and the socket server and client (`serve.rs`).

## Platform layer

`src/sys.rs` wraps the libc calls grfsh needs (fork, exec, pipes, process groups, terminal control, wait) and turns errors into `io::Error`. `src/signals.rs` records signals in atomics from an async signal safe handler, and the executor acts on them between commands. Handlers are installed without `SA_RESTART`, so a blocking wait returns when a signal arrives.

## Safety checks

`src/trust.rs` refuses startup files and plugins that another user could have changed. `src/check.rs` implements `--check` on top of the parser.

## Tests

- Unit tests live next to the code.
- `tests/integration.rs` runs the binary on scripts.
- `tests/compat.rs` compares `tests/compat/*.sh` with recorded bash 5 output.
- `tests/interactive.rs` drives the shell through a pseudo terminal.
- `tests/agent.rs` covers agent mode, the session server and the editing and search tools.
- `fuzz/` holds the cargo-fuzz targets.
