set -e; echo a; false; echo b
#---
set -e; false || true; echo survived; if false; then :; fi; echo s2; ! true; echo s3
#---
set -e; f() { false; echo inf; }; f; echo after
#---
set -e; x=$(false); echo after
#---
set -e; false && true; echo after
#---
set -e; (false); echo after
#---
set -e; false | true; echo after
#---
set -eo pipefail; false | true; echo after
#---
set -u; echo ${undef:-ok}; echo $undef; echo after
#---
set -u; set -- ; echo "$@" ok
#---
set -x; echo traced 2>/dev/null
#---
set -euo pipefail; echo strict
#---
set -o | grep -E '^(errexit|nounset|pipefail)' | awk '{print $1,$2}'
#---
set -e; set +e; false; echo continued
#---
echo $- | grep -q e && echo has_e || echo no_e
#---
trap 'echo bye' EXIT; echo main
#---
trap 'echo bye' EXIT; exit 3
#---
f() { trap 'echo ret' RETURN; }; f; echo x
#---
trap 'echo caught' USR1; kill -USR1 $$; echo after
#---
trap 'echo int' INT; kill -INT $$; echo after
#---
trap 'echo err at $LINENO' ERR; false; echo after
#---
trap - EXIT; trap 'echo one' EXIT; trap 'echo two' EXIT
#---
trap '' TERM; kill -TERM $$; echo ignored
#---
trap 'echo t' EXIT; (echo sub; exit 0); echo main
#---
trap -p EXIT; trap 'echo z' EXIT; trap -p EXIT
#---
sleep 10 & pid=$!; kill $pid; wait $pid; echo rc=$?
#---
(sleep 0.1; exit 3) & wait $!; echo rc=$?
#---
sleep 0.1 & sleep 0.1 & wait; echo all
#---
for i in 1 2 3; do (echo $i) & done; wait
#---
sleep 0.05 & jobs | wc -l | tr -d ' '
#---
timeout 1 sleep 3; echo rc=$?
#---
eval 'x=5; echo $x'; cmd='echo "a b"'; eval "$cmd"
#---
eval echo '$'HOME | sed "s|$HOME|H|"
#---
echo 'echo sourced $1; y=from_src' > s.sh; . ./s.sh arg; echo $y; source s.sh again
#---
echo 'return 5; echo no' > r.sh; . ./r.sh; echo rc=$?
#---
type echo cd ls | sed 's|/.*bin/|BIN/|'
#---
command -v ls git >/dev/null && echo found; command -v nopecmd; echo rc=$?
#---
type -t echo; type -t ls; f(){ :; }; type -t f; type -t if
#---
command echo cmd; builtin echo bi
#---
echo() { builtin echo "wrapped $*"; }; echo hi; command echo raw
#---
which ls >/dev/null && echo w
#---
hash -r; echo hashed
#---
nopecmd 2>/dev/null; echo rc=$?
#---
/etc/hosts 2>/dev/null; echo rc=$?
#---
shopt -s expand_aliases; alias ll='echo LL'
ll
#---
alias g='echo G'; alias g; unalias g; alias g 2>/dev/null || echo unaliased
#---
cd /tmp; cd /; cd - >/dev/null; pwd
#---
cd /usr; cd ..; pwd; cd; [ "$PWD" = "$HOME" ] && echo home
#---
cd /nope 2>/dev/null; echo rc=$?
#---
pushd /tmp >/dev/null; pushd /usr >/dev/null; popd >/dev/null; pwd; dirs | wc -w | tr -d ' '
#---
mkdir -p a/b; cd a/b; echo ${PWD##*/}; cd ../..; echo $OLDPWD | sed 's|.*/||'
#---
CDPATH=/; cd usr >/dev/null; pwd
#---
f() { while getopts "ab:c" o; do case $o in a) echo A;; b) echo B=$OPTARG;; c) echo C;; ?) echo bad;; esac; done; shift $((OPTIND-1)); echo rest=$*; }; f -a -b val -c x y
#---
f() { while getopts ":x:" o; do case $o in :) echo missing $OPTARG;; \?) echo unknown $OPTARG;; esac; done; }; f -q; f -x
#---
f() { OPTIND=1; while getopts ab o; do echo $o; done; }; f -ab; f -b
#---
export E1=v; bash -c 'echo $E1'; env | grep -c '^E1='
#---
E2=temp env | grep ^E2; echo "${E2:-unset}"
#---
export -n X 2>/dev/null; X=1; export X; export -p | grep -c 'X='
#---
declare -i n=5; n+=3; echo $n; declare -r ro=1; ro=2 2>/dev/null; echo rc=$? $ro
#---
readonly R=1; R=2; echo after
#---
declare -x DX=1; bash -c 'echo $DX'; declare -p DX
#---
declare -l low=ABC; declare -u up=abc; echo $low $up
#---
local x=1 2>/dev/null; echo rc=$?
#---
unset PATH; ls 2>/dev/null; echo rc=$?
#---
x=1; unset x; echo ${x-gone}; unset -v nonexist; echo ok
#---
umask 022; umask; umask -S
#---
ulimit -n >/dev/null && echo ulimit_ok
#---
r=$RANDOM; [ $r -ge 0 ] && [ $r -le 32767 ] && echo rand
#---
s=$SECONDS; [ "$s" -ge 0 ] && echo secs
#---
echo ${HOSTNAME:+host} ${UID:+uid} ${EUID:+euid} ${PPID:+ppid}
#---
echo "$OSTYPE" | grep -q darwin && echo darwin
#---
shopt -s extglob 2>/dev/null; echo ok
#---
x=1; : ${x:=2}; : > colon; echo $x
#---
true && { echo grp; echo g2; }
#---
exec echo replaced; echo not
#---
(exit 300); echo $?
#---
exit
#---
false; exit
#---
basename /a/b/c.txt .txt; dirname /a/b/c.txt
#---
time true 2>/dev/null; echo t
#---
{ time sleep 0.01; } 2>&1 | grep -c real
#---
caller 2>/dev/null; echo c
#---
f() { echo ${FUNCNAME[0]} ${#FUNCNAME[@]}; }; f
#---
echo "${BASH_SOURCE[0]:-none}"
#---
help cd >/dev/null 2>&1 && echo help
#---
kill -l | head -c 20 | tr -d '\n' | wc -c | tr -d ' '
#---
kill -0 $$ && echo alive
#---
wait 99999; echo rc=$?
#---
printf '%s\n' c a b | sort -r | head -1
#---
true | false | true; echo ${PIPESTATUS[@]}
#---
! false; echo $?; ! true; echo $?
#---
echo a | { read v; echo $v; }
#---
x=1 y=2; echo $x$y
#---
VAR=one bash -c 'echo $VAR'; f(){ echo $VAR2; }; VAR2=tmp f; echo "[$VAR2]"
