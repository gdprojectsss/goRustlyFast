for i in 1 2 3; do echo $i; done
n=0
while [ $n -lt 3 ]; do n=$((n+1)); done; echo $n
until [ $n -eq 0 ]; do n=$((n-1)); done; echo $n
for i in 1 2 3 4 5; do
  [ $i -eq 2 ] && continue
  [ $i -eq 4 ] && break
  echo "i=$i"
done
for o in a b; do for i in 1 2; do [ $i = 2 ] && continue 2; echo $o$i; done; done
