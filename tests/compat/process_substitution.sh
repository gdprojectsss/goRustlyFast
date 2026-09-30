diff <(printf 'a\nb\n') <(printf 'a\nb\n') && echo same
while read -r line; do echo "got $line"; done < <(printf 'x\ny\n')
cat <(echo inline)
