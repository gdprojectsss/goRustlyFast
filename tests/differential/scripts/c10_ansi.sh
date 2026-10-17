echo $'a\tb\nc'
echo $'\x41é\'q'
echo $'\e[0m' | od -c | head -1
x=$'line1\nline2'; echo "${#x}"
echo $"hi"
