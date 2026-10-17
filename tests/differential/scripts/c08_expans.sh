a=(x "y z" "")
echo "${#a[@]}"; printf '<%s>' "${a[@]}"; echo
printf '<%s>' "${a[@]:1}"; echo
printf '<%s>' "${a[@]/y/Y}"; echo
printf '<%s>' "${a[@]^}"; echo
s="Hello World"
echo "${s,,} ${s^^} ${s:6} ${s: -5:3} ${s/o/0} ${s//o/0} ${s#*o} ${s##*o} ${s%o*} ${s%%o*}"
echo "${#s} ${s/#H/J} ${s/%d/D}"
v=; echo "${v:-def} ${v-unset} ${u:-x} ${u:=y} $u ${v:+alt} ${s:+alt}"
echo "${s@Q}"; q="it's"; echo "${q@Q}"
p_a=1 p_b=2; echo "${!p_*}"; echo "${!p_@}"
ref=s; echo "${!ref}"
echo "${s@U} ${s@L}"
arr=(1 2 3); echo "${arr[-1]} ${arr[@]: -2}"
