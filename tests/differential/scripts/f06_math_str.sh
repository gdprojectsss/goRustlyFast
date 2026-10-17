n="08"; echo $(( 10#$n + 1 ))
echo $(( RANDOM % 1 ))
x=" 5 "; echo $(( x + 1 ))
echo $(( 3 > 2 && 1 ))
echo $(( ~0 )) $(( !0 )) $(( 5 ^ 3 ))
echo $(( 9223372036854775807 + 1 ))
echo $(( 1 / 0 )) 2>/dev/null; echo "after div0 rc=$?"
