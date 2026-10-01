v="a b	c,d"
echo "${v//[[:space:]]}" "${v//[[:punct:]]/_}"
case 42 in [[:digit:]][[:digit:]]) echo two-digits;; esac
[[ X == [[:upper:]] ]] && echo upper
