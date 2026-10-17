declare -A h
h["key with space"]=spaced
echo "${h["key with space"]}"
h['single q']=single
echo "${h['single q']}" "${h[single q]}"
key="key with space"
echo "${h[$key]}"
h[plain]=p
echo "${!h[@]}"
declare -A g=(["a b"]=1 ['c d']=2 [e]=3)
echo "${g["a b"]} ${g['c d']} ${g[e]}"
echo "${!g[@]}"
declare -A m=([b]=2 [a]="x y" [c]='q"z')
declare -p m
for k in "${!m[@]}"; do echo "$k=${m[$k]}"; done
m[a]=updated
echo "${!m[@]}"
unset 'm[a]'
m[a]=again
echo "${!m[@]}"
declare -A big
for ((i = 0; i < 2100; i++)); do big[k$i]=$i; done
keys=("${!big[@]}")
echo "${#big[@]} ${keys[*]:0:6}"
declare -A odd=(['x"y']=2 ['$z']=3 ['q]r']=5 [$'n\nl']=6 ['*']=7 ['a=b']=9)
declare -p odd
echo "${odd[@]@K}"
h[""]=empty
echo "not reached"
echo "status after empty key: $?"
