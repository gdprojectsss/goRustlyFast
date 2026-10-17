declare -A m=([b]=2 [a]="x y" [c]='q"z')
declare -p m
declare -a ar=(1 "" 'a b')
declare -p ar
declare -A e
declare -p e
e=()
declare -p e
declare -a ea
declare -p ea
ea=()
declare -p ea
declare -i i=3
declare -p i
declare -l lo=ABC
declare -p lo
declare -u U=abc
declare -p U
declare -lrx z=A
declare -p z
declare -iux w=3
declare -p w
declare -ai ai=(1 2)
declare -p ai
declare s2='a$b`c"\d'
declare -p s2
declare nl=$'a\nb'
declare -p nl
declare esc=$'tab\there\e[0m'
declare -p esc
declare ctl=$'\x01\x7f'
declare -p ctl
declare quote=$'quote\'s and \\ back'
declare -p quote
declare -r ro=1
declare -p ro
declare un
declare -p un
un=1
declare -p un
declare -i iu
declare -p iu
declare -a sp
sp[3]=x
sp[1]=y
declare -p sp
declare -p nosuch
echo "missing status $?"
f() {
    local lv=1
    declare -p lv
    local unset_local
    echo "${unset_local-unset}"
    declare -p unset_local
}
f
x=5
g() { local x; echo "local shadows: ${x-unset}"; }
g
echo "global again: $x"
x=(a b c)
unset 'x[1]'
declare -p x
unset x
declare -p x 2>/dev/null
echo "unset array status $?"
declare -n nr
declare -p nr
nr=x
declare -p nr
readonly RO1=2
readonly -p | grep RO1
