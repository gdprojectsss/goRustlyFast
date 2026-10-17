sleep 10 & p=$!
kill -s TERM $p; echo "kill-s rc=$?"
sleep 10 & kill -15 %1 2>/dev/null; echo "kill %1 rc=$?"
sleep 10 & kill -l | head -1 | grep -q HUP && echo kill-l
kill -l 15
kill $(jobs -p) 2>/dev/null; echo done
