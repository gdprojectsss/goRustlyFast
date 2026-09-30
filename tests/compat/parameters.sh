set -- one "two three" four
echo $#
echo "$1|$2|$3"
for a in "$@"; do echo "<$a>"; done
for a in "$*"; do echo "<$a>"; done
shift
echo "$@"
unset missing
echo "${missing:-fallback} ${missing-dash} ${empty=assigned} $empty"
v=hello
echo "${v:+alt} ${#v} ${v:1:3} ${v: -2}"
