for w in apple Banana cherry "" 12 -x; do
case $w in
  [aA]*) echo "a: $w";;
  [[:upper:]]*) echo "upper: $w";;
  ch*|xx) echo "ch: $w" ;&
  fall) echo "fell";;
  "") echo empty;;
  [0-9][0-9]) echo num;;
  -*) echo opt;;&
  *) echo "default $w";;
esac
done
