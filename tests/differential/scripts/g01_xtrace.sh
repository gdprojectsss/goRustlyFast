set -x
x=1
echo "a b" $x
f() { echo in; }; f
set +x
echo done
