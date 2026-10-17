echo a > f; echo b >> f; cat f
echo err 2>&1 >/dev/null | cat
{ echo out; echo err >&2; } 2>&1 | sort
{ echo out; echo err >&2; } &>both; sort both
{ echo out; echo err >&2; } &>>both; wc -l < both
echo x 1>&2 2>/dev/null
cat <<< "herestring"
cat < /dev/null; echo rc=$?
echo hi >| clob; cat clob
exec 4<>rw; echo rwdata >&4; exec 4>&-; cat rw
ls /nonexistent_zz 2>&1 | grep -c 'No such' 
echo "to stderr" >&2 2>/dev/null
cmd_out=$( { echo o; echo e >&2; } 2>&1 ); echo "$cmd_out" | wc -l
echo data | tee f2 >/dev/null; cat f2
> empty; [[ -s empty ]] || echo empty
echo abc |& cat
