echo 'single $HOME' "double \$x" back\ slash
x="a  b   c"
echo $x
echo "$x"
printf '[%s]\n' "$x" $x
echo "nested 'quotes' and \"escapes\""
echo $'not-ansi-c' 2>/dev/null || true
