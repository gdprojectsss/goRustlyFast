cmd="echo 'single' \"double\" \$HOME"; eval "$cmd" | sed "s|$HOME|H|"
args=(-l "a b"); printf '<%s>' "${args[@]}"; echo
str='a "b c" d'; eval "set -- $str"; echo $#
q=$(printf '%q' "x y\$z"); eval "echo $q"
v='$(echo injected)'; echo "$v"
echo '\n' "\n" \n
echo "a\"b" 'c'"'"'d' "e\$f" "g\\h" "i\`j"
echo "${HOME:+"quoted alt"}"
x="a*b"; echo "$x" ${x}
echo \$\{x\}
