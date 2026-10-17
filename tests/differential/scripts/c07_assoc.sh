declare -A m
m["a b"]=1; m[x-y]=2; m["k=v"]=3; m['$x']=4; m[""]=5 2>/dev/null; m[1]=6
for k in "${!m[@]}"; do echo "$k=${m[$k]}"; done | sort
echo "${#m[@]}"
[[ -v m["a b"] ]] && echo has
unset 'm[a b]'; echo "${#m[@]}"
declare -A n=([one]=1 [two]=2); echo "${n[two]}"
