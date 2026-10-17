cd "$(mktemp -d)"
touch one.txt
echo hello > *.txt; cat one.txt
touch two.txt
echo x > *.txt 2>/dev/null; echo "ambiguous $?"
exec 5>moved.txt; echo moved >&5; exec 6>&5-; echo again >&6; exec 6>&-; cat moved.txt
{ echo out; echo err >&2; } >&both.txt; sort both.txt
f() { (exit 3); }
trap 'echo "err at status $?"' ERR
f; echo "after f $?"
trap - ERR
trap 'echo dbg' DEBUG
x=1
trap - DEBUG
trap 'echo usr' USR1
echo "$(trap -p USR1)"
( trap -p USR1 )
kill -l 0
echo done
