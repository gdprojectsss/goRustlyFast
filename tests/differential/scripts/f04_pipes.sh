echo a | while read x; do y=$x; done; echo "y=${y-unset}"
shopt -s lastpipe; echo b | read z; echo "z=${z-unset}"
cat /dev/null | wc -l
seq 1 100000 | head -1
yes | head -2
printf '%s\n' c a b | sort | tr '\n' ' '; echo
echo $(echo a; echo b)
echo "$(echo a; echo b)"
cnt=$(seq 5 | wc -l | tr -d ' '); echo $cnt
