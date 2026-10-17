declare -A m
printf -v 'm[key]' '%s' val 2>/dev/null; echo "${m[key]-none}"
printf -v v '%s,' a b c; echo "${v%,}"
declare -p v
declare -A z=([a]=1); declare -p z
arr=(x "y z"); declare -p arr
