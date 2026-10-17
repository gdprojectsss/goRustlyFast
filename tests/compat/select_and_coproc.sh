PS3="pick: "
select f in apple banana cherry; do
  echo "f=[$f] REPLY=[$REPLY]"
  [[ $f == apple ]] && break
done <<EOF
2

9
x
1
EOF
echo "select status $?"
select f in a b; do
  echo "got $f"
done <<EOF
1
EOF
echo "eof status $?"
set -- p q
select f; do echo "positional $f"; break; done <<< 2
select f in; do echo never; done
echo "empty status $?"
select f in a b; do echo "[$f] [$REPLY]"; break; done <<< " 2 "
coproc cat
echo hello >&"${COPROC[1]}"
read -r line <&"${COPROC[0]}"
echo "got $line"
[[ -n $COPROC_PID ]] && echo "has pid"
exec {COPROC[1]}>&-
wait
coproc UPPER { tr a-z A-Z; }
echo shout >&"${UPPER[1]}"
exec {UPPER[1]}>&-
read -r line <&"${UPPER[0]}"
echo "$line"
wait
