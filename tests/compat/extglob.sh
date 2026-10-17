[[ a == @(a|b) ]] && echo "conditional always extended"
p='+([0-9])'
[[ 123 == $p ]] && echo "pattern from variable"
[[ "@(a)" == "@(a)" ]] && echo "quoted is literal"
[[ a == "@(a)" ]] || echo "quoted does not match"
x="   hi"
echo "off: [${x##+([[:space:]])}]"
shopt -s extglob
for w in a ab abc abcd "" b c cc; do
  case "$w" in
    ?(a)) echo "optional:$w" ;;
    +(a|b)) echo "plus:$w" ;;
    *(c)) echo "star:$w" ;;
    @(abc|abcd)) echo "at:$w" ;;
    *) echo "other:$w" ;;
  esac
done
case foo.c in !(*.h)) echo "not a header" ;; esac
[[ x == !(y) ]] && echo 1
[[ "" == !(y) ]] && echo 2
[[ y == !(y) ]] || echo 3
[[ abc == a!(b)c ]] || echo 4
[[ axc == a!(b)c ]] && echo 5
[[ a.b.c == *.@(c|d) ]] && echo 6
[[ abab == *(ab) ]] && echo 7
y="  lead  trail  "
echo "[${y##+( )}] [${y%%+( )}] [${y//@(a|e)/_}]"
z=foo.bar.baz
echo "[${z//!(.)/-}] [${z%%.!(*.*)}] [${z##*([^.]).}]"
w=aaabbb
echo "${w//+(a)/X} ${w/#*(a)/Y}"
dir=${TMPDIR:-/tmp}/grfsh_compat_extglob_$$
mkdir -p "$dir"
(
  cd "$dir" || exit
  touch a.c b.h c.txt .hidden.c
  echo !(*.h)
  echo @(a|b).*
  echo *.+(c|h)
  echo ?(a).c
  echo @(nomatch|none)
)
rm -rf "$dir"
shopt -u extglob
