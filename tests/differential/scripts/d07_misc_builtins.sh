echo -n "no newline"; echo
echo -e "a\tb"; echo -E 'a\tb'
echo "-n"; echo -- "-x"
pushd /tmp >/dev/null; popd >/dev/null; echo pushok
dirs >/dev/null && echo dirs
cd /tmp && cd - >/dev/null && echo cdback
umask 022; umask
ulimit -n >/dev/null && echo ulimit
times >/dev/null 2>&1; echo times=$?
read -t 1 x </dev/null; echo readt=$?
read -n 3 x <<< "abcdef"; echo $x
read -s -p "" y <<< "secret"; echo $y
read -d , z <<< "aa,bb"; echo $z
shift 2>/dev/null; echo shiftrc=$?
alias ll='ls -l'; alias ll; unalias ll
enable -n echo 2>/dev/null; echo enable=$?
compgen -c ls 2>/dev/null | head -1
declare -i n=5; n+=2; echo $n; n="3*4"; echo $n
declare -u up=hello; echo $up
declare -l lo=HELLO; echo $lo
declare -r RO=1; echo $RO
declare -x EXP=1; bash -c 'echo exp=$EXP'
typeset t=typeset; echo $t
declare -n nr=up; echo $nr; nr=changed; echo $up
readonly -p | grep -c RO >/dev/null && echo rop
export -n EXP; bash -c 'echo unexp=${EXP-none}'
set -u; echo "${undef-}"; ( echo $undef2 ) 2>/dev/null || echo "nounset err"
