a=1 b=$a; echo "b=$b"
x=1 y=2 env | grep -E '^(x|y)=' | sort
z=3 true; echo "z=${z-unset}"
f() { echo "in f: ${w-unset}"; }; w=tmp f; echo "w after: ${w-unset}"
export E1=1 E2; E2=2; /opt/homebrew/bin/bash -c 'echo "$E1 $E2"'
declare -A m; m[a]+=x; m[a]+=y; echo ${m[a]}
s=abc; s+=def; echo $s
arr=(1 2); arr[5]=6; echo "${!arr[@]}"
i=3; i+=1; echo $i
declare -i j=3; j+=1; echo $j
readonly r=1; r=2 2>/dev/null; echo "still r=$r"
