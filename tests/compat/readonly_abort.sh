readonly R=1
unset R
echo "unset status $?"
unset -v R
echo "unset -v status $?"
R=2; echo "same line not reached"
echo "next line status $?"
(R=2; echo "not reached in subshell")
echo "subshell status $?"
(R=2 echo "prefix still runs")
(declare R=3; echo "declare status $?")
(f() { R=4; echo "not reached in function"; }; f; echo "not reached after function")
echo "function subshell status $?"
for R in a; do echo "loop body"; done; echo "for status $?"
(R+=x; echo "not reached after append")
(readonly list=(1 2); list[0]=5; echo "not reached after element")
(read R <<< x; echo "read status $?")
(export R=9; echo "export status $?")
(x=1 R=2 y=3; echo "not reached after multiple")
{ R=2; echo "not reached in group"; }
echo "after group"
f() { R=2; }
f || echo "not reached or"
echo "after function"
readonly -f no_such_function
echo "readonly -f status $?"
unset 1bad
echo "invalid unset status $?"
declare -A h
h[""]=x
echo "not reached after empty key"
echo "empty key status $?"
echo end
