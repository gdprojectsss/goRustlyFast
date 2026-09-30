printf 'one two three\n' | { read a b; echo "[$a] [$b]"; }
printf 'x:y:z\n' | { IFS=: read -r p q; echo "$p|$q"; } 2>/dev/null || echo skip
printf 'l1\nl2\n' | { while read line; do echo "<$line>"; done; }
printf 'a b c\n' | { read -a arr; echo ${#arr[@]} ${arr[1]}; }
