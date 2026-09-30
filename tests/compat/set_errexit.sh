set -e
if false; then echo no; fi
false || echo "or ok"
false && echo no
echo "still running"
f() { false; echo "in f after false"; }
f || true
echo end
