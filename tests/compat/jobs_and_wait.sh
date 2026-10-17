true &
sleep 0.2
wait $!
echo "remembered $?"

(exit 3) &
pid=$!
sleep 0.2
wait $pid
echo "exited $?"
wait $pid
echo "again $?"

wait 1
echo "not a child $?"

sleep 0.1 &
(exit 4) &
wait -n
echo "next $?"
wait -n
echo "next $?"
wait -n
echo "none left $?"

(exit 5) &
pid=$!
wait -p found -n
echo "wait -p $?"
[ "$found" = "$pid" ] && echo "found the pid"

sleep 1 &
false &
sleep 0.3
jobs
echo "running:"
jobs -r
echo "stopped:"
jobs -s
echo "after listing:"
jobs
kill %1
wait %1
echo "killed $?"

sleep 1 &
disown
jobs
echo "disowned $?"
kill $!
disown %7
echo "missing job $?"

sleep 1 &
kill -s TERM %1
wait
echo "wait without arguments $?"

f() ( exit 3 )
f
echo "subshell function $?"
g() { ( exit 4 ); }
g
echo "group function $?"

times | wc -l | tr -d ' '

sleep 1 &
echo "explicit subshell:"
(jobs)
echo "pipeline:"
jobs | cat
echo "substitution: $(jobs)"
kill %1
wait
