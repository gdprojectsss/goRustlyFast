(( a = 1, a += 2, a *= 2 )); echo "comma $a"
for (( i = 0, j = 10; i < 3; i++, j-- )); do echo "$i $j"; done
echo $(( "3" + 4 ))
echo $(( 1, 2, 3 ))
arr=(1 5 9)
(( arr[1] > 2 )) && echo big
echo $(( arr[0] + arr[1] ))
i=2
(( arr[i]++ ))
echo "${arr[2]}"
(( arr[3] = 7 ))
echo "${arr[@]}"
(( ++arr[0] )); echo "${arr[0]}"
(( arr[1] += 10 )); echo "${arr[1]}"
declare -A m
m[k]=4
echo $(( m[k] * 2 ))
k=zz; m[zz]=3
echo $(( m[$k] + 1 ))
b=a; a=5; echo $(( b + 1 ))
x=foo; foo=3; echo $(( x * 2 ))
e="1+2"; echo $(( e * 3 ))
r=r2; r2=r3; r3=4; echo $(( r ))
s=" 3 "; echo $(( s + 1 ))
[[ 08 -eq 8 ]]; echo "octal error status $?"
[[ 1+1 -eq 2 ]] && echo "conditions use arithmetic"
echo $(( 10#08 )) $(( 0x1f )) $(( 2#101 )) $(( 8#17 ))
echo $(( 0 && (1/0) )) $(( 1 || (1/0) )) $(( 1 ? 2 : 1/0 ))
c=0; echo $(( 0 && c++ )) $c
echo $(( 1 ++ 2 )) $(( 7 % -3 )) $(( -7 / 2 )) $(( 1 << 65 ))
echo $(( 9223372036854775807 + 1 ))
echo $(( 3 > 2 > 1 )) $(( !!5 )) $(( ~0 )) $(( 5 ^ 3 )) $(( 6 & 3 | 8 ))
n=5; (( n /= 2, n <<= 3 )); echo $n
unset u; echo $(( u++ )) $u
sparse[5]=2; echo $(( sparse )) $(( sparse[5] ))
echo $(( )) empty
echo before $(( 1 / 0 )); echo same line
echo "after division status $?"
(( 5 / 0 )); echo "command status $?"
f() { echo in; echo $(( 1 % 0 )); echo still; }
f; echo after f
echo end
