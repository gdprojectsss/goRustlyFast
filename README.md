# grfsh

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org)

**[English](#english) | [Romana](#romana)**

---

## English

A Unix shell written in Rust. Fast, minimal, and no bloat.

### Features

- Pipes, `&&`, `||`, `;`, `!`, subshells `( )` and groups `{ }`
- Redirections on any fd: `>`, `>>`, `<`, `2>`, `2>&1`, `&>`, `>&-`, heredocs `<<EOF` / `<<-EOF` and here strings `<<<`
- Expansion when the command runs, not when it is parsed: `$VAR`, `$?`, `$$`, `$#`, `$@`, `$1`..., `${x:-default}`, `${x:=value}`, `${x:+alt}`, `${x:?error}`, `${#x}`, `$(...)`, backticks, `$((arithmetic))`, `~`, globs with `*`, `?` and `[a-z]`
- Scripting: `if`/`elif`/`else`, `while`, `until`, `for`, functions with `local` and `return`, `break`/`continue N`, `read`, `source`, `eval`
- Job control: `cmd &`, `jobs`, `fg`, `bg`, `wait`, `kill %1`, Ctrl-Z to stop, Ctrl-C to interrupt (loops included)
- Table pipelines: `files`, `procs`, `load`, `where`, `sort-by`, `select`, `to json` and friends (see below)
- `z` to jump to frequently used directories, and typing a directory name changes into it
- "did you mean" suggestions for mistyped commands
- Tab completion that knows about git subcommands and branches, cargo, ssh hosts, make targets, `$VARS`, `%jobs` and directories after `cd`
- History based autosuggestions, syntax highlighting, multiline input
- Prompt with git branch, exit status of the last command, its duration when slow, and the number of background jobs
- Aliases, persistent history (`~/.grfsh_history`) and an RC file (`~/.grfshrc`)

### Build

```
cargo build --release
```

The binary lands in `target/release/grfsh`.

### Usage

```
grfsh                          interactive shell
grfsh script.sh [args...]      run a script
grfsh -c 'command' [name args] run one command line
```

```
$ ./target/release/grfsh
danie@host:~/projects/grfsh (main) $ ls src | grep lib
lib.rs
danie@host:~/projects/grfsh (main) $ for f in src/*.rs; do wc -l < $f; done
danie@host:~/projects/grfsh (main) $ sleep 60 &
[1] 4242
danie@host:~/projects/grfsh (main) jobs:1 $ gti status
grfsh: gti: command not found, did you mean 'git'?
danie@host:~/projects/grfsh (main) jobs:1 [127] $ z proj
```

### Table pipelines

Table commands pass typed rows to each other, so filters understand sizes, numbers and dates. Printed to a terminal they show an aligned table, piped anywhere else they turn back into plain text.

```
$ files src/table | where size -gt 8kb | sort-by size -r
name        type    size  modified
----------  ----  ------  -------------
filters.rs  file  9.4 KB  1 minute ago
json.rs     file  9.1 KB  3 minutes ago

$ procs | sort-by mem -r | first 5 | select pid name mem
$ files | where modified gt 1d | get name | xargs ls -l
$ load deps.csv | where license == MIT | to json
$ ps aux | where %CPU gt 5
```

| Command | What it does |
| --- | --- |
| `files [path] [-a]` | directory listing with name, type, size, modified |
| `procs` | running processes with pid, ppid, cpu, mem, rss, name |
| `load <file>` | reads `.csv`, `.tsv`, `.json`, `.jsonl`, anything else as lines |
| `from csv\|tsv\|json\|jsonl\|ws\|lines` | parses text coming from a pipe |
| `where <column> <op> <value>` | keeps matching rows, ops: `== != =~ !~ eq ne lt le gt ge` |
| `sort-by <column>... [-r]` | sorts by one or more columns |
| `select` / `reject <column>...` | keeps or drops columns |
| `first [n]`, `skip [n]`, `reverse` | slices rows |
| `count`, `get <column>` | prints the row count or one column as plain lines |
| `to csv\|tsv\|json\|md\|table` | converts the table to text |

`<` and `>` are redirections in any shell, so write `where size -gt 1mb` or quote the whole condition: `where 'size > 1mb'`. Values follow the column type: `10mb` for sizes, `2d` or `2026-01-31` for dates. Output from regular commands is parsed as JSON or as columns separated by whitespace with a header line, like the output of `ps`. To reach a system command with the same name as a table command, use `command`, for example `command procs`.

### Configuration

Create `~/.grfshrc` to run commands on startup:

```sh
export EDITOR=nvim
alias ll='ls -la'
alias gs='git status'

mkcd() { mkdir -p "$1" && cd "$1"; }
```

Lines starting with `#` are comments.

Set `GRFSH_PROMPT` to customize the prompt. Placeholders: `{user}`, `{host}`, `{cwd}`, `{branch}`, `{git}`, `{status}`, `{duration}`, `{jobs}` and the colors `{green}`, `{blue}`, `{magenta}`, `{cyan}`, `{red}`, `{yellow}`, `{bold}`, `{reset}`.

```sh
export GRFSH_PROMPT='{blue}{cwd}{reset} {git}{red}{status}{reset}> '
```

### Builtins

`cd`, `z`, `pwd`, `echo`, `exit`, `export`, `unset`, `local`, `set`, `shift`, `read`, `source`, `.`, `eval`, `alias`, `unalias`, `type`, `command`, `return`, `break`, `continue`, `jobs`, `fg`, `bg`, `wait`, `kill`, `true`, `false`, `:`

### Project layout

```
src/
  syntax/    lexer, parser and AST
  expand/    word expansion, arithmetic, globbing
  exec/      running commands, pipelines, redirections, processes
  builtins/  builtin commands
  table/     typed values, table commands, JSON/CSV, rendering
  repl/      prompt, completion, highlighting, the interactive loop
```

### Dependencies

[`rustyline`](https://crates.io/crates/rustyline) for line editing and [`libc`](https://crates.io/crates/libc) for process groups, signals and file descriptors. Everything else is stdlib.

### License

MIT

---

## Romana

Un shell Unix scris in Rust. Rapid, minimal, fara prostii in plus.

### Ce stie sa faca

- Pipe-uri, `&&`, `||`, `;`, `!`, subshell-uri `( )` si grupuri `{ }`
- Redirectari pe orice fd: `>`, `>>`, `<`, `2>`, `2>&1`, `&>`, `>&-`, heredoc `<<EOF` / `<<-EOF` si here string `<<<`
- Expandare in momentul rularii, nu la parsare: `$VAR`, `$?`, `$$`, `$#`, `$@`, `$1`..., `${x:-implicit}`, `${x:=valoare}`, `${x:+alt}`, `${x:?eroare}`, `${#x}`, `$(...)`, backtick, `$((aritmetica))`, `~`, glob-uri cu `*`, `?` si `[a-z]`
- Scripting: `if`/`elif`/`else`, `while`, `until`, `for`, functii cu `local` si `return`, `break`/`continue N`, `read`, `source`, `eval`
- Job control: `cmd &`, `jobs`, `fg`, `bg`, `wait`, `kill %1`, Ctrl-Z ca sa opresti, Ctrl-C ca sa intrerupi (si bucle)
- Pipeline-uri cu tabele: `files`, `procs`, `load`, `where`, `sort-by`, `select`, `to json` si altele (vezi mai jos)
- `z` ca sa sari in directoarele folosite des, iar daca scrii doar numele unui director intri in el
- Sugestii "did you mean" cand gresesti o comanda
- Tab completion care stie subcomenzi si branch-uri git, cargo, host-uri ssh, target-uri make, `$VARIABILE`, `%job-uri` si doar directoare dupa `cd`
- Autosugestii din istoric, syntax highlighting, input pe mai multe linii
- Prompt cu branch-ul git, codul de iesire al ultimei comenzi, cat a durat cand e lenta si numarul de job-uri in fundal
- Alias-uri, istoric persistent (`~/.grfsh_history`) si fisier RC (`~/.grfshrc`)

### Compilare

```
cargo build --release
```

Binarul ajunge in `target/release/grfsh`.

### Cum se foloseste

```
grfsh                          shell interactiv
grfsh script.sh [args...]      ruleaza un script
grfsh -c 'comanda' [nume args] ruleaza o singura linie
```

```
$ ./target/release/grfsh
danie@host:~/projects/grfsh (main) $ ls src | grep lib
lib.rs
danie@host:~/projects/grfsh (main) $ echo salut > out.txt && cat out.txt
salut
danie@host:~/projects/grfsh (main) $ gti status
grfsh: gti: command not found, did you mean 'git'?
danie@host:~/projects/grfsh (main) [127] $ z proj
```

### Pipeline-uri cu tabele

Comenzile de tabel isi dau unele altora randuri cu tipuri, asa ca filtrele inteleg dimensiuni, numere si date. In terminal apar ca un tabel aliniat, iar cand le trimiti mai departe printr-un pipe devin text simplu.

```
$ files src/table | where size -gt 8kb | sort-by size -r
$ procs | sort-by mem -r | first 5 | select pid name mem
$ files | where modified gt 1d | get name | xargs ls -l
$ load deps.csv | where license == MIT | to json
$ ps aux | where %CPU gt 5
```

| Comanda | Ce face |
| --- | --- |
| `files [cale] [-a]` | listare de director cu name, type, size, modified |
| `procs` | procesele active cu pid, ppid, cpu, mem, rss, name |
| `load <fisier>` | citeste `.csv`, `.tsv`, `.json`, `.jsonl`, restul ca linii |
| `from csv\|tsv\|json\|jsonl\|ws\|lines` | parseaza text venit printr-un pipe |
| `where <coloana> <op> <valoare>` | pastreaza randurile potrivite, operatori: `== != =~ !~ eq ne lt le gt ge` |
| `sort-by <coloana>... [-r]` | sorteaza dupa una sau mai multe coloane |
| `select` / `reject <coloana>...` | pastreaza sau scoate coloane |
| `first [n]`, `skip [n]`, `reverse` | taie din randuri |
| `count`, `get <coloana>` | afiseaza numarul de randuri sau o coloana ca linii simple |
| `to csv\|tsv\|json\|md\|table` | transforma tabelul in text |

`<` si `>` sunt redirectari in orice shell, deci scrie `where size -gt 1mb` sau pune toata conditia intre ghilimele: `where 'size > 1mb'`. Valorile urmeaza tipul coloanei: `10mb` pentru dimensiuni, `2d` sau `2026-01-31` pentru date. Iesirea comenzilor obisnuite e parsata ca JSON sau ca si coloane separate prin spatii cu un rand de header, cum scoate `ps`. Daca ai o comanda de sistem cu acelasi nume ca o comanda de tabel, foloseste `command`, de exemplu `command procs`.

### Configurare

Fa un fisier `~/.grfshrc` cu ce vrei sa ruleze la pornire:

```sh
export EDITOR=nvim
alias ll='ls -la'
alias gs='git status'

mkcd() { mkdir -p "$1" && cd "$1"; }
```

Liniile care incep cu `#` sunt comentarii.

Seteaza `GRFSH_PROMPT` ca sa-ti faci promptul cum vrei. Placeholder-e: `{user}`, `{host}`, `{cwd}`, `{branch}`, `{git}`, `{status}`, `{duration}`, `{jobs}` si culorile `{green}`, `{blue}`, `{magenta}`, `{cyan}`, `{red}`, `{yellow}`, `{bold}`, `{reset}`.

### Built-in-uri

`cd`, `z`, `pwd`, `echo`, `exit`, `export`, `unset`, `local`, `set`, `shift`, `read`, `source`, `.`, `eval`, `alias`, `unalias`, `type`, `command`, `return`, `break`, `continue`, `jobs`, `fg`, `bg`, `wait`, `kill`, `true`, `false`, `:`

### Dependinte

[`rustyline`](https://crates.io/crates/rustyline) pentru line editing si [`libc`](https://crates.io/crates/libc) pentru grupuri de procese, semnale si file descriptori. In rest, totul e din stdlib.

### Licenta

MIT
