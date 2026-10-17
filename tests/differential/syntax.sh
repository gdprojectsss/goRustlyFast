echo a # comment
echo b#notcomment
#---
echo one \
  two \
  three
#---
echo "multi
line"; echo 'multi
line2'
#---
if true
then
  echo yes
fi
#---
f()
{
  echo brace-next-line
}
f
#---
function h() { echo both; }; h
#---
echo if then else fi do done case esac in
#---
echo {a,b} '{a,b}' "{a,b}" {a} {} {,} x{,}y
#---
echo a{1..3}b {3..1} {a..c}{1..2} {z..x}
#---
echo !notexpanded "a!b"
#---
x=hello; echo "${x}world" "$x"world "${x}_" $x_
#---
echo $1x "$9" ${10} 
#---
set -- a b c d e f g h i j k; echo ${10} $10
#---
x="  lead and trail  "; echo "[$x]"; echo [$x]; y=$x; echo "[$y]"
#---
IFS=: ; x="a:b::c"; for i in $x; do echo "[$i]"; done
#---
IFS=' :'; x="a : b"; set -- $x; echo $#
#---
x="a
b"; for i in $x; do echo "[$i]"; done
#---
IFS=; x="a b"; for i in $x; do echo "[$i]"; done
#---
unset IFS; x="a	b  c"; set -- $x; echo $#
#---
x=$'a\tb'; echo "$x" | od -c | head -1
#---
a="x y"; b=$a; c="$a"; echo "$b|$c"
#---
var=~/x; echo $var | sed "s|$HOME|H|"; var2="~/x"; echo $var2
#---
PATH2=~/a:~/b; echo $PATH2 | sed "s|$HOME|H|g"
#---
x=*; echo "$x"; touch g1; echo $x
#---
echo "$(echo "a   b")"; echo $(echo "a   b")
#---
echo "${x:-"default with space"}" ${y:-'single'}
#---
x="a'b"; echo "${x}" "$x" '$x'
#---
echo "\$ \` \\ \" \a \n"
#---
echo 'a\nb'; echo "a\nb"
#---
echo $((  1 +
  2 ))
#---
echo $(( (2+3) * (4-1) ))
#---
x=$(( 0x1F + 010 )); echo $x
#---
a=5; b=a; echo $((b+1))
#---
echo $(( $(echo 3) + 1 ))
#---
x=10; ((x--)); echo $x; ((--x)); echo $x; ((x=x*2, x+1)); echo $x
#---
echo $((-5 / 2)) $((-5 % 2)) $((5 / -2))
#---
echo $(( 9223372036854775807 + 1 ))
#---
(( 5 > 3 )) && echo gt; (( 0 )) || echo zero
#---
x=; echo $((x+1))
#---
echo $(( "3" + 4 ))
#---
{ echo a; echo b; } | wc -l | tr -d ' '
#---
(echo a; exit 3); echo $?
#---
{ false; }; echo $?
#---
if false; then :; fi; echo $?
#---
for i in; do :; done; echo $?
#---
false; for i in a; do :; done; echo $?
#---
false; while false; do :; done; echo $?
#---
case x in y) ;; esac; echo $?
#---
x=$(false) ; echo $?; x=1 ; echo $?
#---
false; x=$(true); echo $?
#---
f(){ false; }; if f; then echo t; else echo f; fi
#---
echo a && false || echo recovered
#---
true || false && echo and
#---
echo a | false; echo $?
#---
echo a; ; echo b
#---
echo a;; 
#---
)
#---
if true; then echo x
#---
echo "unclosed
#---
echo $(( 1 + ))
#---
echo ${x
#---
fi
#---
cat <<EOF
no terminator
#---
done
#---
{ echo a }
#---
echo a & echo b; wait
#---
echo a&&echo b||echo c
#---
echo a|tr a b
#---
echo a>out;cat out
#---
echo 2>&1 3>&1 x
#---
echo a 1>&2 2>/dev/null | wc -c | tr -d ' '
#---
x=1; { x=2; }; echo $x; (x=3); echo $x
#---
x=1; echo a | { read x; }; echo $x
#---
cd /tmp && (cd /usr; pwd) && pwd
#---
true; ( exit $? ); echo $?
#---
exit 0 | echo pipe; echo after
#---
$(echo echo) dynamic
#---
cmd=echo; $cmd hello; "$cmd" quoted
#---
x="echo a b"; $x; "$x" 2>/dev/null; echo rc=$?
#---
empty=; $empty echo ran
#---
"" 2>/dev/null; echo rc=$?
#---
a=1 b=$a; echo $b
#---
a=1; a=2 b=$a; echo $b
#---
export A=1 B; B=2; bash -c 'echo $A$B'
#---
x[0]=a x[1]=b; echo ${x[@]}
#---
echo ${#undefined_arr[@]}
#---
declare -a arr=(1 2); declare -p arr
#---
x=(a b c); echo ${x}; echo $x
#---
x=(a b c); unset x; echo ${#x[@]}
#---
arr=("a b" c); printf '[%s]' "${arr[@]}"; echo; printf '[%s]' ${arr[@]}; echo
#---
arr=(); printf '[%s]' "${arr[@]}"; echo end
#---
set --; printf '[%s]' "$@"; echo end
#---
s='a,b,c'; IFS=, read -r x y z <<< "$s"; echo $z
#---
s='a b'; arr=($s "$s"); echo ${#arr[@]}
#---
echo ${arr[@]:-empty}
#---
a=(1 2 3); echo $(( ${a[0]} + ${a[2]} ))
#---
a=(5 3 9); echo ${a[@]:1:1}
#---
a=(x y z); echo "${!a[@]}"
#---
a=([2]=b [0]=a); echo ${a[@]}
#---
declare -A h=([one]=1 [two]=2); echo ${h[two]}
#---
declare -A h; h["key with space"]=v; echo "${h["key with space"]}"
#---
declare -A h; [[ -v h[x] ]] || echo unset_key; h[x]=1; [[ -v h[x] ]] && echo set_key
#---
x=1; [[ -v x ]] && echo setx; [[ -v nope ]] || echo unsetnope
#---
s=héllo; echo ${#s}
#---
s=日本語; echo ${s:1:1} ${#s}
#---
printf '%s\n' "ünïcödé" | tr a-z A-Z
#---
s="CamelCase"; echo ${s,,} ${s^^}
#---
x=abc; echo ${x@Q} ${x@U} 2>/dev/null
#---
pre_a=1 pre_b=2; echo ${!pre_*}
