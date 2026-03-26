# grfsh

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org)

**[English](#english) | [Romana](#romana)**

---

## English

A Unix shell written in Rust. Fast, minimal, and no bloat.

### Features

- Pipes and redirections (`|`, `>`, `>>`, `<`)
- Command chaining (`&&`, `||`, `;`)
- Quoting, escaping, and `$VAR` expansion
- Tab completion for commands and file paths
- Persistent history (`~/.grfsh_history`)
- Colored zsh-style prompt with git branch detection
- Aliases with defaults for `ls` and `grep` color output
- RC file support (`~/.grfshrc`)

### Build

```
cargo build --release
```

The binary lands in `target/release/grfsh`.

### Usage

```
$ ./target/release/grfsh
danie@host:~/projects/grfsh (main) $ ls src | grep parser
parser.rs
danie@host:~/projects/grfsh (main) $ echo hello > out.txt && cat out.txt
hello
danie@host:~/projects/grfsh (main) $ alias ll='ls -la'
danie@host:~/projects/grfsh (main) $ exit
```

### Configuration

Create `~/.grfshrc` to run commands on startup:

```sh
export EDITOR=nvim
alias ll='ls -la'
alias gs='git status'
```

Lines starting with `#` are comments.

### Builtins

`cd`, `exit`, `export`, `echo`, `alias`, `unalias`

### Dependencies

Just [`rustyline`](https://crates.io/crates/rustyline) for line editing. Everything else is stdlib.

### License

MIT

---

## Romana

Un shell Unix scris in Rust. Rapid, minimal, fara prostii in plus.

### Ce stie sa faca

- Pipe-uri si redirectari (`|`, `>`, `>>`, `<`)
- Inlantuire de comenzi (`&&`, `||`, `;`)
- Ghilimele, escape si expandare de variabile `$VAR`
- Tab completion pe comenzi si path-uri
- Istoric persistent (`~/.grfsh_history`)
- Prompt colorat gen zsh cu branch-ul git curent
- Alias-uri (cu `ls` si `grep` colorate din start)
- Suport fisier RC (`~/.grfshrc`)

### Compilare

```
cargo build --release
```

Binarul ajunge in `target/release/grfsh`.

### Cum se foloseste

```
$ ./target/release/grfsh
danie@host:~/projects/grfsh (main) $ ls src | grep parser
parser.rs
danie@host:~/projects/grfsh (main) $ echo salut > out.txt && cat out.txt
salut
danie@host:~/projects/grfsh (main) $ alias ll='ls -la'
danie@host:~/projects/grfsh (main) $ exit
```

### Configurare

Fa un fisier `~/.grfshrc` cu ce vrei sa ruleze la pornire:

```sh
export EDITOR=nvim
alias ll='ls -la'
alias gs='git status'
```

Liniile care incep cu `#` sunt comentarii.

### Built-in-uri

`cd`, `exit`, `export`, `echo`, `alias`, `unalias`

### Dependinte

Doar [`rustyline`](https://crates.io/crates/rustyline) pentru line editing. In rest, totul e din stdlib.

### Licenta

MIT
