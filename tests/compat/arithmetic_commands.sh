x=5
(( x > 3 )) && echo big
(( x == 9 )) || echo not-nine
for ((i = 0; i < 3; i++)); do printf '%d ' $i; done; echo
total=0
for ((i = 1; i <= 10; i++)); do (( total += i )); done
echo $total
(( n = 2 ** 8 )); echo $n
