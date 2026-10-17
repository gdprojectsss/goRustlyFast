printf -v x '%05d' 42; echo "$x"
printf -v arr[3] '%s' hi; echo "${arr[3]}"
printf '%s\n' "${x}" | cat
printf '%q\n' "a b'c"
printf '%b\n' 'a\tb'
printf '%-5s|%5s|\n' ab cd
printf '%x %o %e\n' 255 8 1.5
printf '%(%Y)T\n' 0
printf '%s %s\n' 1 2 3
printf '%d\n' "'A"
