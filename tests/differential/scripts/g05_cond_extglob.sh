x=b
[[ $x == @(a|b) ]] && echo m1
[[ foo123 == foo+([0-9]) ]] && echo m2
[[ $x != !(b) ]] && echo m3
