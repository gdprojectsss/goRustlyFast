x=1; [[ -v x ]] && echo set; [[ -v nope ]] || echo unset
a=(1); [[ -v a[0] ]] && echo a0; [[ -v a[5] ]] || echo no5
declare -A m=([k]=1); [[ -v m[k] ]] && echo mk
[[ -n ${x+x} ]] && echo plus
