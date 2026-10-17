printf 'a\nb\n\n\n' > f
x=$(< f); echo "[$x]"
x="$(<f)"; echo "${#x}"
mapfile -t L < f; echo "${#L[@]} ${L[1]}"
readarray -t M < <(printf '1\n2\n'); echo "${M[@]}"
