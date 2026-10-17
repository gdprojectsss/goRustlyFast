set -eE
trap 'echo "ERR at line $LINENO rc=$?"' ERR
f() { echo in-f; false; }
f
echo notreached
