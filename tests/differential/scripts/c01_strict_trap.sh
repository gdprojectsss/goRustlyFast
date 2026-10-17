set -euo pipefail
tmp=$(mktemp -d)
cleanup() { rc=$?; rm -rf "$tmp"; echo "cleanup rc=$rc"; }
trap cleanup EXIT
echo hi > "$tmp/f"
cat "$tmp/f"
false
echo notreached
