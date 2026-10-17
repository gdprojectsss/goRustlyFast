printf 'a\nb\n' > f
IFS= read -r -d '' content < f; echo "rc=$? [${content}]"
mapfile -d '' -t arr < <(printf 'x\0y z\0'); echo "${#arr[@]} ${arr[1]}"
mapfile -t -s 1 -n 1 L < <(printf '1\n2\n3\n'); echo "${L[@]}"
mapfile -t -C 'echo cb' -c 1 L2 < <(printf 'q\n') 2>/dev/null; echo "${L2[0]}"
