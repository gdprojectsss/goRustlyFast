IFS=, read -r a b c <<< "1,2,3,4"; echo "$a|$b|$c"
IFS=: read -ra parts <<< "a:b::c"; echo "${#parts[@]} ${parts[2]}"
s="a b  c"; set -- $s; echo $#
IFS=,; s="x,y,,z"; set -- $s; echo $#; unset IFS
IFS=$'\n' lines=($(printf 'l 1\nl 2\n')); echo "${#lines[@]}"; unset IFS
old=$IFS; IFS=; v="a b"; for w in $v; do echo "w=$w"; done; IFS=$old
read -r x <<< "  lead trail  "; echo "[$x]"
IFS= read -r x <<< "  lead trail  "; echo "[$x]"
echo "a  b" | { read -r p q; echo "$p/$q"; }
read -r -a arr <<< "1 2 3"; echo "${arr[2]}"
