<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/logo-dark.svg">
  <img alt="grfsh" src=".github/assets/logo-light.svg" width="340">
</picture>

### Un shell Unix mic, scris in Rust.<br>Scripting in stil bash, job control, pipeline-uri care transporta tabele cu tipuri si un mod pentru agenti.

<p>
  <a href="https://github.com/gdprojectsss/goRustlyFast/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/gdprojectsss/goRustlyFast/ci.yml?branch=main&style=flat-square&label=ci"></a>
  <a href="https://www.rust-lang.org"><img alt="Rust 2024 edition" src="https://img.shields.io/badge/rust-2024_edition-f97316?style=flat-square&logo=rust&logoColor=white"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-6366f1?style=flat-square"></a>
  <a href="https://github.com/gdprojectsss/goRustlyFast/releases"><img alt="Versiunea 1.0" src="https://img.shields.io/badge/version-1.0-f97316?style=flat-square"></a>
  <a href="https://grf.sh/docs/"><img alt="Documentatie" src="https://img.shields.io/badge/docs-grf.sh-0ea5e9?style=flat-square"></a>
</p>

<p>
  <a href="#pornire-rapida"><b>Pornire rapida</b></a> |
  <a href="https://grf.sh/docs/"><b>Documentatie</b></a> |
  <a href="#pipeline-uri-cu-tabele"><b>Pipeline-uri cu tabele</b></a> |
  <a href="#pentru-agenti"><b>Pentru agenti</b></a> |
  <a href="#de-ce-grfsh"><b>De ce grfsh</b></a> |
  <a href="#roadmap"><b>Roadmap</b></a> |
  <a href="README.md"><b>English</b></a>
</p>

<img alt="sesiune grfsh in terminal" src=".github/assets/demo.svg" width="880">

</div>

<br>

> [!NOTE]
> grfsh 1.0 ruleaza scripturi reale si e placut de folosit interactiv, dar nu acopera tot ce face bash: lipsesc extglob, referintele de nume, coprocesele si `select`. Ruleaza `grfsh --check script.sh` ca sa vezi daca un script foloseste ceva ce grfsh nu stie, iar lista completa e la [compatibilitate](https://grf.sh/docs/compatibility/).

## Ce e grfsh?

grfsh foloseste sintaxa din bash pentru cazurile obisnuite: pipe-uri, redirectari, `if`/`for`/`while`, functii, `$(...)`, `${x:-implicit}`, `$((calcule))`, heredoc si job control.

Pentru folosirea interactiva are autosugestii din istoric, syntax highlighting, completion pentru branch-uri git, host-uri ssh si target-uri make, sugestii cand gresesti o comanda, o comanda `z` pentru directoarele folosite des si un prompt care arata codul de iesire si durata ultimei comenzi.

Mai are si un set de comenzi de tabel. `files`, `procs` si `load` scot tabele cu tipuri, asa ca `where size -gt 10mb` compara dimensiuni, iar `where modified gt 2d` compara date. Cand un tabel ajunge la `grep` sau `wc`, e afisat ca text simplu.

Pentru agenti de programare si joburi de CI are un mod pentru agenti: fara pager-e sau intrebari, output tinut in buget, rezultate JSON, un server de sesiune persistent, `timeout`, `explain` si editari exacte de fisiere.

Depinde de doua crate-uri: `rustyline` pentru line editing si `libc` pentru procese si semnale.

## Pornire rapida

```sh
curl -fsSL https://grf.sh | sh
```

Scriptul alege binarul potrivit pentru sistemul tau, verifica suma SHA-256 si il instaleaza in `~/.local/bin`. Exista si pachete `.deb` si `.rpm`, un PKGBUILD pentru AUR, un flake Nix, o formula Homebrew si `cargo install --git https://github.com/gdprojectsss/goRustlyFast`. Detalii la [instalare](https://grf.sh/docs/installation/).

```
grfsh                          shell interactiv
grfsh -l                       login shell, citeste intai ~/.grfsh_profile
grfsh script.sh [args...]      ruleaza un script
grfsh -c 'comanda' [nume args] ruleaza o singura linie
grfsh --check script.sh        arata ce nu poate rula grfsh, fara sa ruleze nimic
grfsh --agent -c 'comanda'     modul pentru agenti, vezi mai jos
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

## Pentru agenti

Agentii, joburile de CI si integrarile din editoare folosesc shell-ul fara nimeni la tastatura. grfsh are din start ce le trebuie:

| Unealta | Ce face |
| --- | --- |
| `--agent` sau `GRFSH_AGENT=1` | pager-ele devin `cat`, culorile si intrebarile git sunt oprite, iar comenzile care incearca sa citeasca din terminal esueaza imediat |
| buget de output | output-ul lung pastreaza primele si ultimele linii, restul e salvat intr-un fisier a carui cale e afisata |
| `--json` | status, stdout, stderr, durata, directorul final si variabilele de mediu schimbate, intr-un singur obiect |
| `--serve socket`, `--client` | un shell care isi pastreaza directorul, variabilele si functiile intre cereri, cu timeout pe cerere |
| `timeout` | opreste tot grupul de procese, merge si pe macOS si cu functii |
| `explain`, `--explain` | arata fiecare comanda dupa expandare fara s-o ruleze si semnaleaza ce e riscant |
| `replace`, `write-file` | editari exacte si atomice, care esueaza cand textul lipseste sau apare de doua ori |
| `search`, `git-status`, `ports` | cautare care respecta `.gitignore` si status ca tabele |

Detalii in [ghidul pentru agenti](https://grf.sh/docs/agents/), iar [grf.sh/llms.txt](https://grf.sh/llms.txt) are aceleasi instructiuni intr-o forma compacta, pentru agenti.

## Performanta

Masurat contra dash, bash si zsh cu [hyperfine](https://github.com/sharkdp/hyperfine), poti reproduce cu `scripts/bench.sh`. Timp mediu pe un Apple M2 cu macOS 26.6, bash 3.2.57 de sistem, dash si zsh 5.9:

| Test | grfsh | dash | bash | zsh |
| --- | ---: | ---: | ---: | ---: |
| Pornire, `-c true` | 2.0 ms | 1.2 ms | 1.6 ms | 2.6 ms |
| Bucla `while [ ]` cu 20.000 de iteratii | 31.6 ms | 37.0 ms | 65.9 ms | 35.1 ms |
| Construirea unui string de 5.000 de ori | 16.6 ms | 50.2 ms | 917.0 ms | 107.8 ms |
| Pipeline cu cinci comenzi | 4.6 ms | 3.7 ms | 4.2 ms | 5.5 ms |
| 200 de command substitutions | 87.0 ms | 77.1 ms | 83.9 ms | 91.7 ms |

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
| Mod pentru agenti cu rezultate JSON          |    da     |  nu  | nu  |  nu  |   nu    |
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

Built-in-uri: `cd`, `z`, `pushd`, `popd`, `dirs`, `pwd`, `printf`, `test`, `[`, `trap`, `timeout`, `replace`, `write-file`, `explain`, `history`, `echo`, `exit`, `export`, `unset`, `local`, `set`, `shift`, `read`, `source`, `.`, `eval`, `alias`, `unalias`, `type`, `command`, `return`, `break`, `continue`, `jobs`, `fg`, `bg`, `wait`, `kill`, `true`, `false`, `:`

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
| 6 | Teste de conformitate si diferentiale, fuzzing, intarire | gata |
| 6 | Release-uri, pachete, pagina man, site de documentatie, ghiduri de migrare | gata |
| 6 | Mod pentru agenti, rezultate JSON, sesiuni, `timeout`, `explain`, `replace`, `search` | gata |

## Contributii

Issue-urile si pull request-urile sunt binevenite, vezi [CONTRIBUTING.md](CONTRIBUTING.md). Inainte sa trimiti o schimbare, ruleaza aceleasi verificari ca CI-ul:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Licenta

[MIT](LICENSE)
