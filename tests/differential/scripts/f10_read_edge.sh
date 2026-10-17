printf 'a\\\nb\n' | { read x; echo "[$x]"; }
printf 'a\\\nb\n' | { read -r x; echo "[$x]"; }
printf 'x y z\n' | { read a b; echo "[$a][$b]"; }
printf '' | { read x; echo "rc=$? [$x]"; }
printf 'nolf' | { read x; echo "rc=$? [$x]"; }
printf 'k=v\n' | { IFS== read -r k v; echo "$k:$v"; }
printf 'one\ttwo\n' | { IFS=$'\t' read -r a b; echo "$a|$b"; }
read -r -p "prompt> " ans <<< "answer"; echo "$ans"
printf '1 2\n3 4\n' | while read -r a b; do echo $((a+b)); done
IFS=, read -r -a f <<< ",a,,b,"; echo "${#f[@]}"
