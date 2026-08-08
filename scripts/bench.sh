#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo build --release --quiet
grfsh="$PWD/target/release/grfsh"

if ! command -v hyperfine >/dev/null 2>&1; then
    echo "hyperfine is required: https://github.com/sharkdp/hyperfine" >&2
    exit 1
fi

shells="$grfsh"
for candidate in dash bash zsh; do
    if command -v "$candidate" >/dev/null 2>&1; then
        shells="$shells $(command -v "$candidate")"
    fi
done

run() {
    name=$1
    script=$2
    set --
    for shell in $shells; do
        set -- "$@" -n "$(basename "$shell")" "$shell -c '$script'"
    done
    echo "== $name"
    hyperfine --warmup 3 --shell=none --export-markdown "target/bench-$name.md" "$@"
}

run startup 'true'
run loop 'i=0; while [ $i -lt 20000 ]; do i=$((i + 1)); done'
run strings 'i=0; s=; while [ $i -lt 5000 ]; do s="$s-$i"; i=$((i + 1)); done'
run pipeline 'printf "%s\n" a b c d e f g h | sort | uniq | wc -l'
run substitution 'i=0; while [ $i -lt 200 ]; do x=$(echo $i); i=$((i + 1)); done'
