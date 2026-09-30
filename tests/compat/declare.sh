declare -i n=5
n+=3
echo $n
declare -u up=shout
echo $up
declare -l low=MiXeD
echo $low
readonly R=1
R=2 2>/dev/null
echo $R
