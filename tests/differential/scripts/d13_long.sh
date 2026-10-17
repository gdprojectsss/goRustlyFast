for i in $(seq 1 5000); do s+="x"; done; echo ${#s}
a=(); for ((i=0;i<20000;i++)); do a+=($i); done; echo ${#a[@]} ${a[19999]}
str=$(printf 'line%d\n' $(seq 1 10000)); echo "${#str}"
