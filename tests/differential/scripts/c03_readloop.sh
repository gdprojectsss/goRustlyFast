printf 'a b\n  c  \nlast' > f
while IFS= read -r line; do echo "[$line]"; done < f
while read -r line || [[ -n $line ]]; do echo "<$line>"; done < f
while read -r a b; do echo "a=$a b=$b"; done < f
