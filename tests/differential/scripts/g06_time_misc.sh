time true 2>/dev/null
{ time sleep 0.01; } 2>&1 | grep -c real
TIMEFORMAT=%R; { time true; } 2>&1 | grep -cE '^[0-9.]+$'
kill -0 $$ && echo self-alive
sleep 5 & p=$!; disown $p 2>/dev/null; kill $p; echo killed
wait -n 2>/dev/null; echo waitn=$?
