set -euo pipefail
tmpd=$(mktemp -d)
trap 'rc=$?; rm -rf "$tmpd"; echo "cleanup $rc" >&2; exit $rc' EXIT
trap 'echo interrupted >&2; exit 130' INT TERM
work() { echo "working in tmp"; cp /nonexistent "$tmpd/" 2>/dev/null; }
work
echo never
