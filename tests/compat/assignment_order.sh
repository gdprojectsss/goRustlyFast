a=1 b=$a
echo "b=$b"
unset a b
a=1 b=$a sh -c 'echo "prefix b=$b"'
echo "a after prefix: ${a-unset}"
x=5
x=7 y=$x sh -c 'echo "y=$y"'
echo "x=$x"
show() { echo "in function b=$b c=$c"; }
c=3 b=$c show
echo "after function: ${b-unset} ${c-unset}"
p=1 p=$((p + 1)) q=$p
echo "p=$p q=$q"
IFS=$'\n' lines=($(printf 'a b\nc d\n'))
echo "${#lines[@]} [${lines[0]}] [${lines[1]}]"
unset IFS
n=1; n+=2 m=$n
echo "n=$n m=$m"
arr=(x) arr+=(y) count=${#arr[@]}
echo "count=$count"
