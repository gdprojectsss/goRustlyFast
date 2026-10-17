sleep 0.1 & pid=$!
wait; echo "wait all rc=$?"
( exit 5 ) & wait $!; echo "wait pid rc=$?"
for i in 1 2 3; do (sleep 0.$i; echo job$i) & done; wait
jobs >/dev/null; echo jobs
sleep 1 & kill $!; wait $! 2>/dev/null; echo "killed rc=$?"
