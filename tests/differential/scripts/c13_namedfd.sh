exec {fd}>out.txt
echo "fd ok" >&$fd
exec {fd}>&-
cat out.txt
exec {r}<out.txt; read -r line <&$r; echo "read: $line"; exec {r}<&-
[[ $fd =~ ^[0-9]+$ ]] && echo numeric
