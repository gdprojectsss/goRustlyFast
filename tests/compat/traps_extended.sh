trap 'echo a' INT TERM
trap 'echo e' 0
trap '' HUP
trap -p
trap -p INT
trap - INT
trap 2>/dev/null
trap -- - TERM HUP EXIT
trap 'echo hi' sigusr2
trap -p USR2
trap foo BOGUS 2>/dev/null
echo "bogus $?"
trap - USR2

kill -l 15
kill -l TERM
kill -l SIGHUP
kill -l 143
kill -l 99 2>/dev/null
echo "invalid $?"

echo "--- ERR without errtrace"
trap 'echo "ERR line $LINENO status $?"' ERR
f() { false; echo "in f"; }
f
g() { false; }
g
echo "--- ERR with errtrace"
set -E
f
g
( false; echo "subshell after" )
set +E
( false )
trap - ERR

echo "--- RETURN"
trap 'echo "RETURN from ${FUNCNAME[0]}"' RETURN
h() { echo in-h; }
h
set -T
h
set +T
k() { trap 'echo inner-return' RETURN; echo in-k; }
k
trap - RETURN

echo "--- DEBUG"
trap 'echo "DEBUG: $BASH_COMMAND"' DEBUG
a=1
echo "a=$a"
m() { echo in-m; }
m
trap - DEBUG

echo "--- subshell EXIT traps"
( trap 'echo subshell exit' EXIT; echo in subshell )
captured=$(trap 'echo substitution exit' EXIT; echo value)
echo "[$captured]"
trap 'echo top exit' EXIT
( echo not inherited )
echo "$(echo nor here)"
