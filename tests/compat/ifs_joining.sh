printf '[%s]\n' "$IFS" | od -c | sed -n 1p
saved=$IFS
IFS=,
set -- "a b" c
echo "$*"
IFS=$saved
echo "$*"
IFS=
echo "$*"
unset IFS
echo "$*"
words=(p "q r" s)
IFS=:
echo "${words[*]}"
echo "${!words[*]}"
echo "${words[*]:1}"
echo "${words[*]/s/S}"
joined=${words[*]}
echo "$joined"
unset IFS
commas() { local IFS=,; echo "$*"; }
commas 1 2 3
echo "outside: $*"
IFS=,
c=$*
echo "unquoted star assignment: $c"
c=$@
echo "unquoted at assignment: $c"
c="$@"
echo "quoted at assignment: $c"
printf '<%s>' $*; echo
printf '<%s>' $@; echo
printf '<%s>' "$*"; echo
printf '<%s>' ${words[*]}; echo
case "$*" in "a b,c") echo "case matched" ;; esac
[[ $* == "a b,c" ]] && echo "conditional matched"
cat <<< "$*"
IFS=
printf '<%s>' "$*" $* "${words[*]}"; echo
unset IFS
local_join() { local m="$@"; echo "m=$m"; }
local_join one two three
items=(x y z)
cat <<< "${items[@]}"
[[ "${items[@]}" == "x y z" ]] && echo "array in conditional"
case "${items[@]}" in "x y z") echo "array in case" ;; esac
set --
printf '<%s>' "$*" "$@"; echo
