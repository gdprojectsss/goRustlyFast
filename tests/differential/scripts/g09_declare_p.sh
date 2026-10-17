x="a'b"; declare -p x
declare -i n=4; declare -p n
declare -x ex=1; declare -p ex
echo "${x@A}"
f() { local l=1; declare -p l; }; f
