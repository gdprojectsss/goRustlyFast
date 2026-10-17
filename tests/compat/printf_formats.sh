printf '%d\n' 3.7 2>/dev/null
echo "status $?"
printf '%d|%d|%i|%x\n' 12abc x -4.2 3.9 2>/dev/null
printf '%d|' '' ' 5' '5 ' 08 0x 0x1G +7 010 2>/dev/null
echo
printf '%d %d\n' 99999999999999999999 -99999999999999999999 2>/dev/null
printf '%f|%g|%.1f\n' 1e3 3.7abc 0.05 2>/dev/null
printf '[%*d]\n' -5 3
printf '[%.*s]\n' -2 hello
printf '[%-*s]\n' -6 ab
printf '[%*s][%-*s]\n' 3 a 3 b
printf '%*d\n' 99999999999 1 2>/dev/null
echo "status $?"
printf '%.*d\n' 99999999999 1 2>/dev/null
echo "status $?"
printf 'x%99999999999d\n' 1 2>/dev/null
echo "status $?"
printf '[%.99999999999s]\n' ab
printf '%ld %lld %hd %5.1Lf %jd %zd\n' 1 2 3 4.25 5 6
printf '%a %A\n' 1 0.5
printf 'a%yb\n' 2>/dev/null
echo "status $?"
printf '%' 2>/dev/null
echo "status $?"
printf '%5' 2>/dev/null
echo "status $?"
printf '%.3d|%5.2s|%-5d|%05d|%#o|%#X|%.3x\n' 7 abc 3 -4 8 255 10
TZ=UTC printf '%(%Y-%m-%d %H:%M:%S)T|\n' 86400
TZ=UTC printf '[%20(%H:%M)T]\n' 3600
TZ=UTC printf '[%-8.2(%H:%M)T]\n' 7200
printf '%(%s)T\n' 1234567890
printf '%q\n' 'a b' "it's" '' 'x$y' '~foo' 'a*b' -x 'a=b' '#x' 'x#y' 'a~b' 'a=~b' 'a:~b' 'a,b' 'a%b' 'a+b' 'a@b' 'a.b/c' '{a}' 'a^b' '#' 'a#' '~'
printf '%q\n' $'a\tb' $'line\nbreak' $'\e[1m' $'\x01x' $'it\'s\ttab'
printf '%Q|%.1Q|\n' ab 'a b'
printf -v plain '%s-%s' a b
echo "$plain"
printf -v 'list[2]' '%s' two
echo "${list[2]} ${#list[@]}"
i=1
printf -v "list[i+3]" '%d' 42
echo "${list[4]}"
declare -A map
printf -v 'map[key with space]' '%s' value
echo "${map[key with space]}"
printf -v 1bad x 2>/dev/null
echo "status $?"
printf -v 2>/dev/null
echo "status $?"
printf -- '%s\n' dashes
printf -v dashed -- '%s' after
echo "$dashed"
