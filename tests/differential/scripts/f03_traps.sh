trap 'echo exit-trap' EXIT
trap 'echo int' INT
trap -p EXIT | head -1
trap - INT
trap '' HUP
trap 'echo usr1' USR1; kill -USR1 $$; sleep 0.1
trap 'echo debug' DEBUG 2>/dev/null; trap - DEBUG
f() { trap 'echo in-f-exit' EXIT; }; f
( trap 'echo sub-exit' EXIT; echo in-sub )
x=$( trap 'echo cs-exit' EXIT; echo cs ); echo "x=[$x]"
exit 3
