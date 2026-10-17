join_by() { local IFS="$1"; shift; echo "$*"; }
join_by , a b c
contains() { local e m=$1; shift; for e; do [[ $e == "$m" ]] && return 0; done; return 1; }
contains b a b c && echo yes
arr=(c a b); IFS=$'\n' sorted=($(sort <<<"${arr[*]}")); unset IFS; echo "${sorted[@]}"
readarray -t s2 < <(printf '%s\n' "${arr[@]}" | sort); echo "${s2[@]}"
uniq_arr=($(printf '%s\n' a b a | sort -u)); echo "${#uniq_arr[@]}"
f() { local -n ref=$1; ref+=(new); }; x=(old); f x 2>/dev/null; echo "${x[@]}"
copy=("${arr[@]}"); copy[0]=Z; echo "${arr[0]} ${copy[0]}"
unset 'arr[1]'; echo "${#arr[@]} ${!arr[@]}"
arr=("${arr[@]}"); echo "${!arr[@]}"
