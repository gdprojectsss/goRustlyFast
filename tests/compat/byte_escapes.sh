hex() { od -An -tx1 | tr -s ' \n' '  '; echo; }
printf '\xc3\xa9' | hex
printf '\xff' | hex
printf '\303\251|\0101|\777|\400' | hex
printf '%b' '\xc3\xa9|\0303\0251|\101|\c ignored' | hex
printf '%.2b|%5b|' 'abc' 'a\tb' | hex
printf '\u00e9\U0001F600\u41|\U110000|\uD800' | hex
printf '\"\?\q\c|\e\E' | hex
echo -e "\033[31m" | hex
echo -e '\0303\0251|\x41\x4|\101|\u00e9|\q\"\?' | hex
echo -e 'stop\cnot' 'shown' | hex
echo -e '\a\b\f\n\r\t\v\\' | hex
echo $'\xc3\xa9|\303\251|\xff|\0101|\cA\c?\c[\c1|\e\E|\"\?\'' | hex
echo $'\u00e9\U0001F600|\U110000|\uD800|\x|\u|end\c' | hex
printf '[%s]' $'nul\0gone' $'nul\x00gone' $'\cZ\c@x' | hex
x=$'\xc3\xa9'; y=$'\xff'; echo "${#x} ${#y}"
z=$'caf\xe9'; echo "${z%$'\xe9'}" "${z/$'\xe9'/E}"
printf '%q %q %q\n' $'\xff' $'a\tb\001' "plain"
printf '%d %d\n' "'é" "'$y"
printf '%b' 'a\x' 2>/dev/null | hex
