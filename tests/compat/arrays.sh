a=(zero one "two words" three)
echo ${#a[@]} ${a[1]} "${a[2]}" ${a[-1]}
a+=(four)
a[10]=ten
echo ${#a[@]} ${!a[@]}
for x in "${a[@]}"; do printf '[%s]' "$x"; done; echo
unset 'a[1]'
echo ${a[@]}
echo "${a[@]:1:2}"
b=()
echo "empty: ${#b[@]}"
