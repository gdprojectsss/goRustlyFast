let x=3+4 y=x*2
echo "$x $y $?"
let 0
echo $?
let "z = 5" "z *= 2"
echo $z
let 2>/dev/null
echo $?
let 'a = 2 ** 3' 'b = a << 1'
echo $a $b
let x++ x++
echo $x $?
let "1/0" 2>/dev/null
echo $?

builtin echo from builtin
builtin nosuch 2>/dev/null
echo $?
builtin
echo $?
cd() { echo "function cd $1"; }
cd /
builtin cd /
pwd
unset -f cd
echo() { builtin echo "wrapped: $*"; }
echo hello
unset -f echo

frame() { local found; found=$(caller "$@") || { echo "no frame $?"; return; }; set -- $found; if [ $# -eq 3 ]; then echo "$1 $2"; else echo "$1"; fi; }
outer() { frame; frame 0; inner; }
inner() { frame 0; frame 1; frame 2; }
outer
first=$(caller); echo "${first%% *} $?"
caller 0
echo "top frame status $?"
caller x 2>/dev/null
echo "invalid status $?"
