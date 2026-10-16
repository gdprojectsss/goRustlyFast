#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
limit="${GRFSH_BENCH_LIMIT:-1.5}"
grfsh="$PWD/target/release/grfsh"
reference="$(command -v dash)"
failed=0

check() {
    name=$1
    script=$2
    hyperfine --warmup 3 --runs 20 --shell=none --style=none \
        --export-json "target/bench-check-$name.json" \
        -n grfsh "$grfsh -c '$script'" \
        -n dash "$reference -c '$script'" > /dev/null
    ratio="$(jq -r 'def mean: .mean // .summary.time_wall_clock.mean;
        (.results[0] | mean) / (.results[1] | mean) * 100 | round / 100' "target/bench-check-$name.json")"
    if awk "BEGIN { exit !($ratio > $limit) }"; then
        echo "$name: grfsh takes ${ratio}x the time of dash, over the ${limit}x limit"
        failed=1
    else
        echo "$name: grfsh takes ${ratio}x the time of dash"
    fi
}

check loop 'i=0; while [ $i -lt 20000 ]; do i=$((i + 1)); done'
check strings 'i=0; s=; while [ $i -lt 5000 ]; do s="$s-$i"; i=$((i + 1)); done'
check substitution 'i=0; while [ $i -lt 200 ]; do x=$(echo $i); i=$((i + 1)); done'
exit "$failed"
