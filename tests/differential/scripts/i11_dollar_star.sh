set -- "a b" c
for x in $@; do echo "u@:$x"; done
for x in "$@"; do echo "q@:$x"; done
for x in $*; do echo "u*:$x"; done
a=("$@"); echo ${#a[@]}
b=($*); echo ${#b[@]}
c="$@"; echo "$c"
