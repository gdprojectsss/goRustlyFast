IFS_SAVE=1
list="a b
c"
for w in $list; do echo "[$w]"; done
empty=""
set -- $empty
echo $#
set -- "$empty"
echo $#
