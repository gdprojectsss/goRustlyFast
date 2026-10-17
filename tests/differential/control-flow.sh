if false; then echo a; elif true; then echo b; else echo c; fi
#---
for ((i=0;i<3;i++)); do echo $i; done
#---
for i in 1 2 3 4 5; do [ $i = 2 ] && continue; [ $i = 4 ] && break; echo $i; done
#---
for i in 1 2; do for j in a b; do [ $j = b ] && continue 2; echo $i$j; done; done
#---
for i in 1 2; do for j in a b; do break 2; done; echo no; done; echo out
#---
i=0; until [ $i -ge 3 ]; do i=$((i+1)); done; echo $i
#---
printf 'a\nb\nc\n' | while read -r l; do echo "got $l"; done
#---
n=0; printf '1\n2\n' | while read l; do n=$((n+1)); done; echo n=$n
#---
n=0; while read l; do n=$((n+1)); done < <(printf '1\n2\n'); echo n=$n
#---
while IFS=: read -r user _ uid _; do echo "$user $uid"; break; done <<< "root:x:0:0"
#---
case "foo.tar.gz" in *.zip) echo zip;; *.tar.*) echo tar;; *) echo other;; esac
#---
case b in a|b|c) echo abc;; esac; case X in [A-Z]) echo upper;; esac
#---
case x in x) echo one;& y) echo two;; z) echo three;; esac
#---
case "" in "") echo empty;; esac
#---
for x; do echo $x; done
#---
set -- p q; for x; do echo $x; done
#---
[ -z "" ] && [ -n x ] && [ a = a ] && [ a != b ] && [ 2 -lt 3 ] && [ 3 -ge 3 ] && echo ok
#---
[[ abc == a* ]] && [[ abc != b* ]] && [[ abc =~ ^a(b)c$ ]] && echo ${BASH_REMATCH[1]}
#---
[[ -e /etc/passwd && -r /etc/passwd && ! -d /etc/passwd ]] && echo ok
#---
[[ "a b" == "a b" ]] && echo q; x="a*"; [[ abc == $x ]] && echo glob; [[ abc == "$x" ]] || echo literal
#---
[[ 10 -gt 9 ]] && [[ 10 > 9 ]] || echo strcmp
#---
re='^[0-9]+$'; [[ 123 =~ $re ]] && echo num
#---
test -f /etc/hosts -a -d /etc && echo both; test ! -e /nope && echo none
#---
[ "$x" = "" ] && echo emptyvar; [ ]; echo rc=$?
#---
[ 1 -eq ]; echo rc=$?
#---
f() { echo "in f: $*"; }; f 1 2; function g { echo g; }; g
#---
f() { local x=inner; echo $x; }; x=outer; f; echo $x
#---
f() { x=changed; }; x=orig; f; echo $x
#---
f() { return 4; }; f; echo $?; f || echo failed
#---
fact() { if [ $1 -le 1 ]; then echo 1; else echo $(( $1 * $(fact $(( $1 - 1 ))) )); fi; }; fact 6
#---
f() { echo "$FUNCNAME"; }; f
#---
f() { cat; }; echo piped | f
#---
f() { echo out; echo err >&2; } ; f 2>/dev/null
#---
f() ( cd /; pwd ); f; pwd | sed "s|$PWD|cwd|"
#---
f() { :; }; type f | head -1; unset -f f; type f >/dev/null 2>&1 || echo gone
#---
echo a > o; echo b >> o; cat o; echo c 1>o; cat o
#---
ls /nonexistent 2>&1 | wc -l | tr -d ' '
#---
{ echo out; echo err >&2; } > all 2>&1; sort all
#---
{ echo out; echo err >&2; } &> all; sort all
#---
{ echo out; echo err >&2; } 2>&1 >/dev/null
#---
echo x >&2 2>/dev/null; echo done
#---
exec 3>fd3; echo via3 >&3; exec 3>&-; cat fd3
#---
exec 4<<<'from4'; read -r v <&4; echo $v
#---
cat <<< "here string $HOME" | sed "s|$HOME|H|"
#---
cat <<-EOT
	tab stripped
	EOT
#---
cat << "EOT" | wc -l
a
b
EOT
#---
x=val; cat <<EOT
$x $(echo cmd) $((1+1)) \$escaped
EOT
#---
echo hi > /dev/null; echo $?; cat /dev/null | wc -c | tr -d ' '
#---
set -o noclobber; echo a > nc; echo b > nc; echo rc=$?; echo c >| nc; cat nc
#---
echo out >o 2>e; ls nope 2>>e; wc -l < e | tr -d ' '
#---
cat < /etc/hosts > /dev/null && echo read
#---
diff <(echo a) <(echo a) && echo same
#---
echo data | tee t1 t2 >/dev/null; cat t1 t2
#---
>empty; wc -c < empty | tr -d ' '
#---
read x y <<< "1 2 3"; echo "$x|$y"
#---
read -r line <<< 'back\slash'; echo "$line"; read line <<< 'back\slash'; echo "$line"
#---
printf 'k=v\n' | { IFS== read -r k v; echo "$k:$v"; }
#---
read -n 3 c <<< "abcdef"; echo $c
#---
read -d , c <<< "ab,cd"; echo $c
#---
echo "no newline" | { read -r a; echo "$a"; }; printf 'nonl' | { read -r a; echo "rc=$? a=$a"; }
#---
read -p "prompt: " a <<< "x" 2>/dev/null; echo $a
#---
read -t 1 a < /dev/null; echo rc=$?
#---
printf '%s-%d-%5.2f-%x-%o-%e\n' str 42 3.14159 255 8 1234.5
#---
printf '%-5s|%5s|%05d\n' ab cd 42
#---
printf '%s\n' a b c
#---
printf '%b\n' 'tab\there'; printf 'pct %% \n'
#---
printf -v out '%03d' 7; echo $out
#---
printf '%q\n' "a b'c"
#---
printf '%c\n' hello; printf '%i\n' 12
#---
printf "%s\n"
#---
echo -n a; echo -e 'b\tc'; echo -E 'd\te'; echo -- -n
#---
echo -ne 'x\n'
