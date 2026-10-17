f() { local -a a=(1 "2 3" 4); local -A m=([k]=v); echo "${#a[@]} ${a[1]} ${m[k]}"; local x; x=5; echo $x; }
f; echo "after:${a[@]-unset}"
g() { local IFS=,; echo "$*"; }; g a b c
declare -a arr; arr+=(x); arr+=(y z); echo "${arr[@]} ${#arr[@]}"
