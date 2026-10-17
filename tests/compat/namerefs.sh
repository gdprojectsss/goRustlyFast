x=5
declare -n r=x
echo "$r"
r=7
echo "$x"
echo "${!r}"
arr=(a b c)
declare -n ra=arr
echo "${ra[1]} ${#ra[@]} ${ra[@]}"
ra[1]=B
echo "${arr[@]}"
ra+=(d)
echo "${arr[@]}"
declare -n element='arr[2]'
echo "$element"
element=C
echo "${arr[@]}"
declare -A h=([k]=v)
declare -n rh=h
echo "${rh[k]}"
rh[z]=w
echo "${h[z]}"
set_it() { local -n out=$1; out="set by function"; }
set_it result
echo "$result"
fill() { local -n list=$1; list=(one two three); }
fill items
echo "${#items[@]} ${items[1]}"
typeset -n tr=x
echo "$tr"
unset -n r
echo "x still: $x"
declare -p r 2>/dev/null || echo "r gone"
declare -n r2=x
unset r2
echo "x after unset through reference: ${x-unset}"
declare -n r3
echo "[${r3}]"
r3=arr
echo "${r3[0]}"
declare -p r3
declare -n loopref
a1=1 a2=2
for loopref in a1 a2; do echo "value $loopref"; done
[[ -v r3 ]] && echo "r3 set"
increment() { local -n counter=$1; (( counter++ )); }
c=1
increment c
echo "c=$c"
declare -n self=self
echo "self reference status $?"
declare -i number=5
declare -n numref=number
declare -p numref number
echo "${numref@a}"
