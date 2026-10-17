touch a.txt b.txt .hidden c.log
echo *.txt
echo *.none
shopt -s nullglob; echo "null:" *.none; shopt -u nullglob
shopt -s dotglob; echo * | tr ' ' '\n' | sort | tr '\n' ' '; echo; shopt -u dotglob
shopt -s extglob; echo !(*.txt) ; echo @(a|b).txt; echo +(a).txt; shopt -u extglob
shopt -s globstar; mkdir -p d/e; touch d/e/f.txt; echo **/*.txt; shopt -u globstar
for f in *.txt; do [[ -e $f ]] || continue; echo "f=$f"; done
[[ a.txt == *.txt ]] && echo match
[[ a.txt == "*.txt" ]] || echo nomatch-quoted
case a.txt in *.@(txt|md)) echo ext;; esac
echo [ab].txt
echo {a,b}.txt {1..3} {01..03} {a..c} {1..10..3}
x=*.txt; echo $x; echo "$x"
shopt -s failglob 2>/dev/null; echo *.zzz; echo after-failglob
