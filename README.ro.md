<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/logo-dark.svg">
  <img alt="grfsh" src=".github/assets/logo-light.svg" width="340">
</picture>

### Un shell Unix mic, scris in Rust.<br>Scripting in stil bash, job control si pipeline-uri care pot transporta tabele cu tipuri.

<p>
  <a href="https://github.com/gdprojectsss/goRustlyFast/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/gdprojectsss/goRustlyFast/ci.yml?branch=main&style=flat-square&label=ci"></a>
  <a href="https://www.rust-lang.org"><img alt="Rust 2024 edition" src="https://img.shields.io/badge/rust-2024_edition-f97316?style=flat-square&logo=rust&logoColor=white"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-6366f1?style=flat-square"></a>
  <img alt="Status: experimental" src="https://img.shields.io/badge/status-experimental-f59e0b?style=flat-square">
</p>

<p>
  <a href="#pornire-rapida"><b>Pornire rapida</b></a> |
  <a href="#pipeline-uri-cu-tabele"><b>Pipeline-uri cu tabele</b></a> |
  <a href="#de-ce-grfsh"><b>De ce grfsh</b></a> |
  <a href="#roadmap"><b>Roadmap</b></a> |
  <a href="README.md"><b>English</b></a>
</p>

<img alt="sesiune grfsh in terminal" src=".github/assets/demo.svg" width="880">

</div>

<br>

> [!NOTE]
> grfsh e un shell tanar. Ruleaza scripturi reale si e placut de folosit interactiv, dar nu acopera inca tot ce face bash (lipsesc extglob, coprocesele si `select`). Ruleaza `grfsh --check script.sh` ca sa vezi daca un script foloseste ceva ce grfsh nu stie.

## Ce e grfsh?

grfsh foloseste sintaxa din bash pentru cazurile obisnuite: pipe-uri, redirectari, `if`/`for`/`while`, functii, `$(...)`, `${x:-implicit}`, `$((calcule))`, heredoc si job control.

Pentru folosirea interactiva are autosugestii din istoric, syntax highlighting, completion pentru branch-uri git, host-uri ssh si target-uri make, sugestii cand gresesti o comanda, o comanda `z` pentru directoarele folosite des si un prompt care arata codul de iesire si durata ultimei comenzi.

Mai are si un set de comenzi de tabel. `files`, `procs` si `load` scot tabele cu tipuri, asa ca `where size -gt 10mb` compara dimensiuni, iar `where modified gt 2d` compara date. Cand un tabel ajunge la `grep` sau `wc`, e afisat ca text simplu.

Depinde de doua crate-uri: `rustyline` pentru line editing si `libc` pentru procese si semnale.

## Pornire rapida

```sh
cargo install --git https://github.com/gdprojectsss/goRustlyFast
grfsh
```

Sau dintr-o clona: `cargo build --release` si apoi `target/release/grfsh`. Exista si o formula Homebrew in [`packaging/homebrew`](packaging/homebrew/grfsh.rb): `brew install --HEAD ./packaging/homebrew/grfsh.rb`.

```
grfsh                          shell interactiv
grfsh -l                       login shell, citeste intai ~/.grfsh_profile
grfsh script.sh [args...]      ruleaza un script
grfsh -c 'comanda' [nume args] ruleaza o singura linie
```

Ca sa-l folosesti ca login shell, adauga calea lui in `/etc/shells` si ruleaza `chsh -s "$(command -v grfsh)"`.

Cateva lucruri de incercat:

```sh
for f in src/*/mod.rs; do echo "$f: $(wc -l < $f) linii"; done
salut() { local cine=${1:-lume}; echo "salut $cine"; }; salut
sleep 30 &                     # apoi: jobs, fg, Ctrl-Z, bg
gti status                     # did you mean 'git'?
z proj                         # sari intr-un director folosit des
git checkout <TAB>             # branch-uri
```

## Pipeline-uri cu tabele

Comenzile de tabel isi dau unele altora randuri cu tipuri. In terminal rezultatul e un tabel aliniat, oriunde altundeva e text simplu pe care `grep`, `awk` si restul il inteleg.

```console
$ load servers.csv | where cpu -gt 50 | sort-by cpu -r
host   region  cpu  uptime_days
-----  ------  ---  -----------
db-1   us       91          203
web-1  eu       72           41

$ ps aux | where %CPU gt 5 | select PID %CPU COMMAND
```

| Comanda | Ce face |
| --- | --- |
| `files [cale] [-a] [-r]` | listare de director cu name, type, size, modified, `-r` recursiv |
| `procs` | procesele active cu pid, ppid, cpu, mem, rss, name |
| `load <fisier>` | citeste `.csv`, `.tsv`, `.json`, `.jsonl`, restul ca linii |
| `from csv\|tsv\|json\|jsonl\|ws\|lines` | parseaza text venit printr-un pipe |
| `where <coloana> <op> <valoare> [and\|or ...]` | pastreaza randurile potrivite, operatori: `== != =~ !~ eq ne lt le gt ge` |
| `sort-by <coloana>... [-r]` | sorteaza dupa una sau mai multe coloane |
| `select` / `reject <coloana>...` | pastreaza sau scoate coloane |
| `first [n]`, `skip [n]`, `reverse` | taie din randuri |
| `count`, `get <coloana>` | afiseaza numarul de randuri sau o coloana ca linii simple |
| `group-by <coloana>` | numara randurile pentru fiecare valoare |
| `uniq-by <coloana>` | pastreaza primul rand pentru fiecare valoare |
| `math sum\|avg\|min\|max\|median <coloana>` | agrega o coloana, dimensiunile raman dimensiuni |
| `update <coloana> <valoare>` | seteaza sau adauga o coloana |
| `to csv\|tsv\|json\|md\|html\|table` | transforma tabelul in text |

`<` si `>` sunt redirectari in orice shell, deci scrie `where size -gt 1mb` sau pune conditia intre ghilimele: `where 'size > 1mb'`. Valorile urmeaza tipul coloanei: `10mb` pentru dimensiuni, `2d` sau `2026-01-31` pentru date.

## Performanta

Masurat contra dash, bash si zsh cu [hyperfine](https://github.com/sharkdp/hyperfine), poti reproduce cu `scripts/bench.sh`. Timp mediu pe un Apple M2 cu macOS 26.6, bash 3.2.57 de sistem, dash si zsh 5.9:

| Test | grfsh | dash | bash | zsh |
| --- | ---: | ---: | ---: | ---: |
| Pornire, `-c true` | 1.9 ms | 1.2 ms | 1.6 ms | 2.5 ms |
| Bucla `while [ ]` cu 20.000 de iteratii | 34.5 ms | 36.7 ms | 65.7 ms | 34.7 ms |
| Construirea unui string de 5.000 de ori | 17.3 ms | 49.8 ms | 911.8 ms | 106.2 ms |
| Pipeline cu cinci comenzi | 4.4 ms | 3.7 ms | 4.2 ms | 5.3 ms |
| 200 de command substitutions | 84.5 ms | 73.2 ms | 83.0 ms | 90.5 ms |

## De ce grfsh?

Cum se compara grfsh cu alte shell-uri la lucrurile pe care se concentreaza:

|                                              | **grfsh** | bash | zsh | fish | nushell |
| -------------------------------------------- | :-------: | :--: | :-: | :--: | :-----: |
| Sintaxa in stil POSIX (`if`, `for`, `$(...)`) |    da     |  da  | da  |  nu  |   nu    |
| Pipe-urile de text merg neschimbate          |    da     |  da  | da  |  da  |   da    |
| Pipeline-uri cu tabele cu tipuri             |    da     |  nu  | nu  |  nu  |   da    |
| Autosugestii din start                       |    da     |  nu  | nu  |  da  |   da    |
| Syntax highlighting din start                |    da     |  nu  | nu  |  da  |   da    |
| Sarit in directoare frecvente, integrat      |    da     |  nu  | nu  |  nu  |   nu    |
| Matur si testat in productie                 |    nu     |  da  | da  |  da  | partial |

<sub>Comparat cu instalarile implicite, fara plugin-uri sau configurari, in octombrie 2026. Corecturile sunt binevenite intr-un issue.</sub>

## Configurare

`~/.grfshrc`, sau `~/.config/grfsh/grfshrc`, ruleaza la pornire. Login shell-urile citesc intai `~/.grfsh_profile`.

```sh
export EDITOR=nvim
alias ll='ls -la'
alias gs='git status'

mkcd() { mkdir -p "$1" && cd "$1"; }
```

Pentru teme ruleaza `configure-prompt`, un wizard scurt cu previzualizari, sau alege direct un preset: `configure-prompt --preset rainbow`. Preset-uri: `lean`, `classic`, `rainbow`, `pure`, `minimal`. Rezultatul e un fisier simplu in `~/.config/grfsh/theme` pe care il poti edita. Ai si hook-uri (`precmd`, `preexec`, `chpwd`), `abbr`, `bindkey`, `complete`, `set -o vi` si plugin-uri in `~/.config/grfsh/plugins/`.

Fara fisier de tema, seteaza `GRFSH_PROMPT` ca sa-ti faci promptul cum vrei. Placeholder-e: `{user}`, `{host}`, `{cwd}`, `{branch}`, `{git}`, `{status}`, `{duration}`, `{jobs}` si culorile `{green}`, `{blue}`, `{magenta}`, `{cyan}`, `{red}`, `{yellow}`, `{bold}`, `{reset}`.

Built-in-uri: `cd`, `z`, `pushd`, `popd`, `dirs`, `pwd`, `printf`, `test`, `[`, `trap`, `history`, `echo`, `exit`, `export`, `unset`, `local`, `set`, `shift`, `read`, `source`, `.`, `eval`, `alias`, `unalias`, `type`, `command`, `return`, `break`, `continue`, `jobs`, `fg`, `bg`, `wait`, `kill`, `true`, `false`, `:`

## Roadmap

| Faza | Ce contine | Status |
| ---- | ---------- | :----: |
| 1 | Comenzi, pipe-uri, redirectari, culori, istoric | gata |
| 2 | Glob-uri, command substitution, syntax highlighting, teste, CI | gata |
| 3 | Parser cu AST, expandare la rulare, scripting, job control | gata |
| 3 | `z`, completion dupa context, "did you mean", prompt mai bogat | gata |
| 3 | Pipeline-uri cu tabele, JSON si CSV | gata |
| 4 | `case`, `[[ ]]`, array-uri, operatori pe parametri, brace expansion | gata |
| 4 | `trap`, `set -euo pipefail`, `printf`, `pushd`, history expansion | gata |
| 4 | Prompt git asincron, benchmark-uri, bucle si string-uri mai rapide | gata |
| 4 | `group-by`, `uniq-by`, `math`, `update`, login shell, release-uri | gata |
| 5 | `getopts`, `declare`, array-uri asociative, `$PIPESTATUS`, glob `**`, process substitution | gata |
| 5 | Teme native, prompt in dreapta si tranzitoriu, `configure-prompt`, hook-uri, mod vi | gata |
| 5 | `grfsh --check`, istoric interogabil ca tabel, plugin-uri, `abbr`, `bindkey` | gata |
| 6 | Teste de conformitate si diferentiale, fuzzing, intarire | planificat |
| 6 | Release-uri, pachete, pagina man, site de documentatie, ghiduri de migrare | planificat |

## Contributii

Issue-urile si pull request-urile sunt binevenite. Inainte sa trimiti o schimbare, ruleaza aceleasi verificari ca CI-ul:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Licenta

[MIT](LICENSE)
