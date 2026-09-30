trap 'echo "cleanup $?"' EXIT
f() { echo in-f; }
f
set -u
echo "${defined_later:-ok}"
