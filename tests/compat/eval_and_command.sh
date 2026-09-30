cmd='echo evaluated $((1+1))'
eval "$cmd"
eval 'x=42'
echo $x
command echo via-command
command -v echo
f() { echo function; }
f
command -v f
