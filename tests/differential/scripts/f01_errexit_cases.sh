set -e
f() { false; echo "f continued (expected in bash when in && context)"; }
f && echo f-ok || echo f-fail
if f; then echo if-ok; fi
x=$(false; echo cmdsub-continued); echo "x=$x"
false || true
! false
local_test() { local v=$(false); echo "local masks rc"; }; local_test
v=$(false) && echo no || echo "assign fail detected"
echo before-last
g() { false; echo "g NOT continued"; }; g
echo notreached
