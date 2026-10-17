#!/bin/sh
set -eu

oils_repository="https://github.com/oils-for-unix/oils.git"
oils_commit="bdc8364c253ce921e033bbc2399ea2ce0be41825"

root="$(cd "$(dirname "$0")/.." && pwd)"
cache="${OILS_SPEC_CACHE:-$root/target/oils-spec}"
checkout="$cache/$oils_commit"

fetch() {
    rm -rf "$checkout.partial"
    mkdir -p "$checkout.partial"
    git -C "$checkout.partial" init -q
    git -C "$checkout.partial" remote add origin "$oils_repository"
    git -C "$checkout.partial" sparse-checkout set --no-cone \
        '/LICENSE.txt' '/spec/*.test.sh' '/spec/bin/' '/spec/testdata/' '/test/sh_spec.py'
    git -C "$checkout.partial" fetch -q --depth 1 --filter=blob:none origin "$oils_commit"
    git -C "$checkout.partial" checkout -q FETCH_HEAD
    mv "$checkout.partial" "$checkout"
}

if [ ! -f "$checkout/LICENSE.txt" ]; then
    echo "fetching the Oils spec tests (Apache License 2.0) at $oils_commit" >&2
    fetch
fi

exec python3 "$root/scripts/spec-runner.py" --oils "$checkout" --work "$cache/run" "$@"
