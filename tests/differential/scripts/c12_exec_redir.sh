exec 3>&1
exec >log 2>&1
echo to-log
echo err-log >&2
exec 1>&3 3>&-
cat log
