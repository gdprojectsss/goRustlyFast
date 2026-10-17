s="version 1.23.4-rc1"
if [[ $s =~ ([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then echo "${BASH_REMATCH[0]} ${BASH_REMATCH[1]} ${BASH_REMATCH[3]} ${#BASH_REMATCH[@]}"; fi
re='^[a-z]+ [0-9.]+'; [[ $s =~ $re ]] && echo m1
[[ $s =~ "1.23" ]] && echo m2
[[ "a.b" =~ a.b ]] && echo m3
[[ "axb" =~ "a.b" ]] || echo m4-literal
[[ foo =~ ^(f)(o+)$ ]] && echo "${BASH_REMATCH[2]}"
[[ x =~ [[:alpha:]] ]] && echo cls
[[ "a b" =~ a\ b ]] && echo spc
