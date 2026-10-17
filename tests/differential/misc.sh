export B; B=2; bash -c 'echo B=$B'
#---
B=2; export B; bash -c 'echo B=$B'
#---
export C=3; C=4; bash -c 'echo C=$C'
#---
dir=/x file=$dir/y; echo $file
#---
touch -t 202001010000 old; touch new; [ new -nt old ] && echo nt; [ old -ot new ] && echo ot
#---
ln -s /etc lnk; [ -L lnk ] && [ -h lnk ] && echo link; echo x > s; [ -s s ] && echo nonempty; [ -s /dev/null ] || echo empty
#---
touch f; chmod 755 f; [ -x f ] && [ -w f ] && [ -r f ] && echo perms; [ -x /bin/ls ] && echo lsx
#---
[ -d / ] && [ -e / ] && [ -f /etc/hosts ] && [ ! -f / ] && [ -c /dev/null ] && [ -p /dev/null ] || echo notpipe
#---
[ -t 0 ] || echo notty; [ -t 1 ] || echo notty1
#---
[ abc \< abd ] && echo lt; [[ b > a ]] && echo gt
#---
[ "a" == "a" ] && echo dbl; [ 01 -eq 1 ] && echo numeq
#---
[[ -z $unset ]] && echo z; [[ $x == "" ]] && echo e
#---
[[ ( -n a ) && ( -z "" || -n b ) ]] && echo grouped
#---
[[ "abc" == *"b"* ]] && echo contains; [[ "a.b" == *.* ]] && echo dot
#---
[[ x =~ [[:alpha:]] ]] && echo cls; [[ "a b" =~ a\ b ]] && echo esc; [[ "ab" =~ "a." ]] || echo literalre
#---
[[ $(echo 5) -eq 5 ]] && echo sub
#---
[ ! -z x -a -n y ] && echo complex; [ \( 1 -eq 1 \) -o 1 -eq 2 ] && echo paren
#---
mkdir real; ln -s real sym; cd sym; pwd; pwd -P | sed 's|.*/||'; cd ..; cd -P sym; pwd | sed 's|.*/||'
#---
mkdir -p r/s; ln -s r/s ls1; cd ls1; cd ..; pwd | sed 's|.*/||'
#---
cd /tmp; echo $PWD; pwd -P
#---
echo $PWD | grep -q '^/' && echo abs
#---
outer() { local v=outer; inner; echo $v; }; inner() { echo seen=$v; v=modified; }; outer
#---
f() { local x; echo "[$x]"; x=1; }; x=global; f; echo $x
#---
f() { local -i n=3; n+=2; echo $n; }; f
#---
f() { local a=(1 2 3); echo ${#a[@]}; }; f
#---
rec() { local n=$1; [ $n -eq 0 ] && return; rec $((n-1)); echo $n; }; rec 3
#---
f() { echo "$@"; shift; echo "$@"; }; f a b c; echo "${@:-none}"
#---
shift 5 2>/dev/null; echo rc=$?
#---
f() { echo $#; }; f "" ""; f
#---
f() { "$@"; }; f echo via args
#---
g() { echo inner $1; }; f() { g "$2"; }; f a b
#---
echo 'echo "0=$0 1=$1 #=$#"' > a.sh; grfsh a.sh x y 2>/dev/null || bash a.sh x y
#---
printf '#!/usr/bin/env bash\necho shebang $1\n' > sb.sh; chmod +x sb.sh; ./sb.sh ok
#---
printf 'echo no shebang\n' > ns.sh; chmod +x ns.sh; ./ns.sh
#---
echo 'echo $BASH_SOURCE ${0##*/}' > bs.sh; source ./bs.sh
#---
mkdir sub; echo 'echo found in path' > sub/pathsrc.sh; PATH="$PWD/sub:$PATH" . pathsrc.sh
#---
. ./missing.sh 2>/dev/null; echo rc=$?
#---
echo $BASHPID >/dev/null; echo ok
#---
echo $$ > p1; (echo $$ > p2); cmp -s p1 p2 && echo same_pid_in_subshell
#---
x=$(cd /; pwd); echo $x; pwd | grep -qv '^/$' && echo unchanged
#---
exec 2>/dev/null; ls /nope; echo after
#---
exec >out; echo hidden; exec >/dev/tty 2>/dev/null; cat out
#---
cat <(echo ps1) <(echo ps2)
#---
echo hi > >(cat); wait; sleep 0.1
#---
while read -r a b; do echo "$b-$a"; done <<EOT
1 one
2 two
EOT
#---
f() { cat <<EOT
in func $1
EOT
}; f arg
#---
x=$(cat <<'EOT'
a $notexp
EOT
); echo "$x"
#---
cat <<A; cat <<B
first
A
second
B
#---
cat <<EOT | tr a-z A-Z
piped heredoc
EOT
#---
cat <<"E O"
spaced
E O
#---
echo $(echo $(echo $(echo deep)))
#---
echo "$(echo "$(echo "q")")"
#---
x=$(printf 'a b\nc'); echo "$x" | wc -l | tr -d ' '
#---
echo $(</etc/hosts) | wc -w | tr -d ' ' | grep -q . && echo slurp
#---
v=$(< /etc/hosts); [ -n "$v" ] && echo slurp2
#---
echo "a" | cat | cat | cat | tr a b
#---
echo -e "b\na\nc" | sort | uniq -c | wc -l | tr -d ' '
#---
seq 1 100000 | tail -1
#---
yes | head -3
#---
seq 5 | while read n; do [ $n = 3 ] && break; echo $n; done
#---
cat /dev/null | read x; echo rc=$?
#---
echo abc | grep -q b && echo found
#---
ls -d / /nope 2>&1 >/dev/null | wc -l | tr -d ' '
#---
printf 'x\0y\0' | xargs -0 echo
#---
find . -maxdepth 0 -print0 | while IFS= read -r -d '' f; do echo "[$f]"; done
#---
echo "a b c" | awk '{print $2}'; echo 'x' | sed -e 's/x/y/'
#---
echo "$(seq 3 | paste -sd+ - | bc)"
#---
git --version | grep -c git
#---
printf '%s' "$(printf '\n\n')" | wc -c | tr -d ' '
#---
i=0; while :; do i=$((i+1)); [ $i -ge 1000 ] && break; done; echo $i
#---
s=; for i in $(seq 200); do s="$s$i,"; done; echo ${#s}
#---
echo ${PIPESTATUS[0]} >/dev/null; false | true | false; echo "${PIPESTATUS[*]}"
#---
command ls -d / ; command -p ls -d /
#---
env -i PATH=/usr/bin:/bin HOME=/tmp printenv HOME
#---
nohup true >/dev/null 2>&1; echo $?
#---
true &; echo bad
#---
sleep 0 & disown; echo disowned
#---
printf '%s %s\n' $(echo a b c d)
#---
printf '%.3s|%5.1s|\n' abcdef xyz
#---
printf '%d\n' 3.7 2>/dev/null; echo rc=$?
#---
printf '%d\n' abc; echo rc=$?
#---
printf '%5s%%\n' 50
#---
printf '\x41\101\n'
#---
printf '%s\n' "${undefined[@]}"
#---
echo -n "" | wc -c | tr -d ' '
