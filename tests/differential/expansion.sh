echo 'single $HOME' "double \$x \" \\ \`" $'ansi\tTAB\n' 
#---
x="a  b"; echo $x; echo "$x"; printf '<%s>' $x "$x" ''; echo
#---
echo "a"'b'c"$(echo d)"
#---
echo \a\b \\ \' \"
#---
x='*'; echo "$x"; echo '$x'
#---
echo "nested \"$(echo "inner \"q\"")\""
#---
printf '%s\n' "line1
line2"
#---
echo $'\x41é\101\e' | od -c | head -2
unset x; echo "${x-unset}" "${x:-empty}"; x=; echo "${x-unset}" "${x:-empty}"
#---
unset x; echo "${x:=dflt}"; echo $x
#---
x=1; echo "${x:+alt}" "${y:+alt}"
#---
( unset x; echo "${x:?is required}" ); echo rc=$?
#---
p=/usr/local/lib/file.tar.gz; echo ${p#*/} ${p##*/} ${p%.*} ${p%%.*}
#---
s=hello; echo ${#s} ${s:1} ${s:1:3} ${s: -2} ${s:(-3):2}
#---
s="a.b.c"; echo ${s/./-} ${s//./-} ${s/#a/X} ${s/%c/Y}
#---
s=Hello; echo ${s^^} ${s,,} ${s^} ${s,}
#---
a=x; b=a; echo ${!b}
#---
s="foo bar"; echo "${s// /_}"
#---
v=abc; echo "${v/b/\/}"
#---
f=file.txt; echo "${f%.txt}.md"
#---
echo ${#} ${#@} $# 
#---
set -- a "b c" d; echo $#; for i in "$@"; do echo "[$i]"; done; for i in $*; do echo "<$i>"; done; echo "$*"
#---
set -- a b c; IFS=,; echo "$*"; IFS=' '
#---
set -- 1 2 3 4 5; shift 2; echo "$@"; echo ${@:2} ${@: -1}
#---
f(){ echo $0 >/dev/null; echo $1 $2 $#; }; f x "y z"
#---
echo $$ | grep -qE '^[0-9]+$' && echo pidok
#---
true; echo $?; false; echo $?; (exit 42); echo $?
#---
echo ${PWD:+set} ${HOME:+set} ${USER:+set} ${PATH:+set}
#---
echo "$LINENO"
#---
x=5; echo $((x*2)) $(( x ** 2 )) $((x%3)) $((x/2)) $((-x)) $((x<<2)) $((x&3)) $((x|8)) $((x^1))
#---
echo $((1+2*3)) $(((1+2)*3)) $((10>3)) $((3==3 && 2!=2)) $((5>3?7:9))
#---
i=0; ((i++)); ((i+=5)); echo $i; ((i>3)) && echo big
#---
echo $((16#ff)) $((2#1010)) $((0x10)) $((010))
#---
x=3; let y=x*4; echo $y
#---
(( 0 )); echo rc=$?
#---
echo $((2**62)) $(( (1<<63)-1 ))
#---
n=07; echo $((10#$n))
#---
echo $(( 7 / 0 )); echo after
a=(one "two three" four); echo ${#a[@]} "${a[1]}" "${a[@]:1}" ${!a[@]}
#---
a=(); a+=(x); a+=(y z); echo ${#a[@]} ${a[-1]}
#---
a=(c b a); for x in "${a[@]}"; do echo $x; done | sort | tr '\n' ' '
#---
a=(1 2 3); a[5]=6; echo ${!a[@]} ${#a[@]}; unset 'a[1]'; echo ${a[@]}
#---
declare -A m; m[k1]=v1; m[k2]=v2; echo ${m[k1]} ${#m[@]}; for k in $(printf '%s\n' "${!m[@]}" | sort); do echo $k=${m[$k]}; done
#---
a=(x y); echo "${a[*]}"; IFS=:; echo "${a[*]}"
#---
read -ra parts <<< "a b c"; echo ${#parts[@]} ${parts[2]}
#---
IFS=, read -ra parts <<< "1,2,3"; echo ${parts[1]}
#---
mapfile -t lines < <(printf 'l1\nl2\n'); echo ${#lines[@]} ${lines[1]}
#---
a=( $(echo "p q r") ); echo ${#a[@]}
#---
a=(a b c); echo "${a[@]/b/B}" "${a[@]^}"
echo {1..5} {a..e} {1..10..3} {05..10} x{a,b}y {a,b}{1,2}
#---
echo ~ ~/x | sed "s|$HOME|H|g"; echo "~" 
#---
touch a.txt b.txt c.log; echo *.txt; echo ?.log; echo [ab].txt; echo *.none
#---
mkdir -p d/e; touch d/e/f.rs d/g.rs; echo d/*/*.rs; echo d/**/*.rs
#---
touch .hidden vis; echo *
#---
touch 'sp ace.txt'; for f in *.txt; do echo "[$f]"; done
#---
shopt -s nullglob; echo x *.zzz y
#---
x='a*'; touch ab ac; echo $x; echo "$x"
#---
set -f; touch q1; echo q*; set +f; echo q*
echo "$(printf 'a\n\n\n')|"
#---
x=$(exit 3); echo $?
#---
echo `echo back` $(echo $(echo two))
#---
out=$(echo err >&2; echo out 2>/dev/null); echo "got:$out"
#---
x=$( cat <<EOT
inside
EOT
); echo $x
#---
echo "$( case x in x) echo matched;; esac )"
#---
d=$(pwd); cd /; cd "$d"; echo ok
