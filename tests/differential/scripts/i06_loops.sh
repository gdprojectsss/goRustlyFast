for i in {1..3}; do for j in a b; do [[ $j == b ]] && continue 2; echo $i$j; done; done
i=0; until (( i >= 3 )); do ((i++)); done; echo $i
for x; do echo "pos $x"; done
while :; do break; done; echo broke
for f in "a b" c; do echo "[$f]"; done
n=0; while read -r; do ((n++)); done <<< $'1\n2\n3'; echo "n=$n REPLY"
for ((i=0,j=10; i<3; i++,j--)); do echo "$i $j"; done
