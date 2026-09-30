#!/bin/sh
set -eu

cd "$(dirname "$0")/../tests/compat"
bash="${BASH_REFERENCE:-bash}"
for case in *.sh; do
    name="${case%.sh}"
    status=0
    "$bash" "$case" > "$name.expected" 2>/dev/null || status=$?
    printf -- '--- exit %s\n' "$status" >> "$name.expected"
done
echo "regenerated $(ls *.expected | wc -l | tr -d ' ') expected outputs with $("$bash" -c 'echo $BASH_VERSION')"
