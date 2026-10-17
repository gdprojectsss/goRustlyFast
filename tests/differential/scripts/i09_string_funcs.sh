trim() { local s="$1"; s="${s#"${s%%[![:space:]]*}"}"; s="${s%"${s##*[![:space:]]}"}"; printf '%s' "$s"; }
echo "[$(trim "   hi there  ")]"
lower=$(echo "ABC" | tr '[:upper:]' '[:lower:]'); echo $lower
repeat() { printf "%${2}s" | tr ' ' "$1"; }; repeat - 5; echo
s="a,b,c"; echo "${s//,/$'\n'}" | wc -l
str="hello"; for ((i=${#str}-1;i>=0;i--)); do r+=${str:i:1}; done; echo $r
echo "${str:0:1}${str:1}" 
[[ "$str" == *ll* ]] && echo contains
x="foo/bar/baz"; IFS=/ read -ra parts <<< "$x"; echo "${parts[-1]}"
