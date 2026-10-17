unset ""; echo "unset empty $?"
unset =5; echo "unset equals $?"
a=([100]=1 2 [5]=a b); declare -p a
declare -A A=(k1 v1 k2 v2); echo "${A[k2]}"
v="y=3"; declare $v; echo "$y"
command declare z=4; echo "$z"
f() { local -a l; echo "${#l[@]}"; }; l=(1 2); f
export X=$'a\0b'; printenv X
echo done
