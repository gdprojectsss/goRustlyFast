show() { printf '<%s>' "$@"; echo; }
target=hello
set -- target other last
echo "indirect 1: ${!1}"
echo "indirect #: ${!#}"
n=2
echo "indirect n: ${!n}"
unset missing
empty=()
one=("")
two=("" "")
single=(a)
show "${missing[@]-d}" "${empty[@]-d}" "${one[@]-d}" "${two[@]-d}" "${single[@]-d}"
show "${missing[@]:-d}" "${empty[@]:-d}" "${one[@]:-d}" "${two[@]:-d}" "${single[@]:-d}"
show "${missing[*]:-d}" "${empty[*]:-d}" "${one[*]:-d}" "${two[*]:-d}" "${single[*]:-d}"
show x "${missing[@]+alt}" "${empty[@]+alt}" "${one[@]+alt}" "${two[@]:+alt}"
set -- "" ""
show "${@:-d}" "${*:-d}" "${@-d}"
set --
show "${@:-d}" "${*:-d}" "${@-d}"
sparse=([3]=x)
show "${sparse[@]-d}" "${sparse-d}"
s=a,b,c
echo "${s//,/$'\n'}"
echo "${s/,/$'\t'}" | od -c | sed -n 1p
v=${s//,/$'-'}
echo "$v"
s=é
echo "${#s}"
LC_ALL=C
echo "C length ${#s}"
unset LC_ALL
LANG=C
echo "LANG=C length ${#s}"
unset LANG
echo "default length ${#s}"
set -- {1..150000}
echo "$# $1 ${150000}"
letters=({a..c}{1..2})
echo "${letters[*]}"
