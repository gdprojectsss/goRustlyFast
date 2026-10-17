indexed=(a b)
[[ -v indexed[1] ]] && echo "indexed 1 set"
[[ -v indexed[5] ]] || echo "indexed 5 unset"
i=1
[[ -v indexed[i] ]] && echo "indexed i set"
[[ -v indexed[i+4] ]] || echo "indexed i+4 unset"
declare -A assoc=([k]=v ["two words"]=w)
[[ -v assoc[k] ]] && echo "assoc k set"
[[ -v assoc[nokey] ]] || echo "assoc nokey unset"
key="two words"
[[ -v assoc[$key] ]] && echo "assoc spaced key set"
[[ -v indexed ]] && echo "indexed set"
[[ -v assoc ]] || echo "assoc without element 0 unset"
[[ -v indexed[@] ]] && echo "indexed has elements"
unset 'indexed[0]'
[[ -v indexed ]] || echo "indexed element 0 unset"
declare declared
[[ -v declared ]] || echo "declared without value unset"
declared=
[[ -v declared ]] && echo "declared empty set"
test -v indexed[1] && echo "test builtin indexed 1 set"
[ -v assoc[k] ] && echo "bracket assoc k set"
