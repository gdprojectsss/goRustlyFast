for ((i=0; i<3; i++)); do echo $i; done
for ((;;)); do ((n++)); ((n>=3)) && break; done; echo n=$n
x=$(( 7/2 )) y=$(( 2**10 )) z=$(( -7 % 3 )) w=$(( 1<<4 | 1 )); echo $x $y $z $w
echo $(( 0x1f + 010 + 2#101 ))
a=5; (( a += 2, a *= 2 )); echo $a
echo $(( a > 3 ? 1 : 0 ))
i=0; echo $((i++)) $((++i)) $i
v=abc; echo $(( v + 1 ))
echo $(( 10 / 3 * 3 ))
