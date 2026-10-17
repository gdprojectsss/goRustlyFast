f() { local n=$1; if (( n == 0 )); then echo 0; return; fi; echo $(( $(f $((n-1))) + 1 )); }
f 50
g() { (( $1 > 0 )) && g $(( $1 - 1 )) || echo done; }; g 2000
fib() { local n=$1; (( n<2 )) && { echo $n; return; }; echo $(( $(fib $((n-1))) + $(fib $((n-2))) )); }; fib 12
