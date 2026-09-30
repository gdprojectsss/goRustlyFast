x=global
f() { local x=f; g; echo "f: $x"; }
g() { echo "g sees: $x"; x=changed-by-g; }
f
echo "after: $x"
h() { local -a list=(1 2 3); echo ${#list[@]}; }
h
echo "list outside: [${list[@]}]"
