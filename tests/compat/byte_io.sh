hex() { od -An -tx1 | tr -s ' \n' '  '; echo; }
data=/tmp/grfsh_compat_bytes_$$
printf 'caf\xe9 na\xefve\nline2 \xff\xfe\n' > "$data"
while IFS= read -r line; do printf '%s|' "$line"; done < "$data" | hex
read -r first second < "$data"; printf '[%s][%s]' "$first" "$second" | hex
echo "${#first}"
mapfile -t lines < "$data"; printf '(%s)' "${lines[@]}" | hex
captured=$(printf '\xff\xfe a\xe9b\n\n'); printf '%s' "$captured" | hex
for word in $captured; do printf '<%s>' "$word"; done | hex
IFS=$'\xe9'; set -- $captured; IFS=' '; printf '{%s}' "$@" | hex
printf '%s' "$(cat "$data")" | hex
printf '%s' "$(< "$data")" | hex
cat <<EOF | hex
$first ${lines[1]}
EOF
cat <<< "$second" | hex
export BYTES_VALUE=$'\xfe\xff'
printenv BYTES_VALUE | hex
declare -p BYTES_VALUE
case $'a\xffb' in a?b) echo matched;; esac
[[ $'\xff' == $'\xff' ]] && echo equal
rm -f "$data"
printf '\xc3\xa9ab' | { read -n 1 x; printf '%s' "$x" | hex; }
printf '\xff\xfeab' | { read -n 2 x; printf '%s' "$x" | hex; }
