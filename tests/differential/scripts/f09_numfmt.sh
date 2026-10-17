printf '%.2f\n' 3.14159
printf '%5.1f|\n' 2.25
printf '%s\n' "$(printf '%03d' 7)"
printf '%c\n' hello
printf '%i\n' 42
printf '%u\n' 42
printf '%%\n'
printf '%*d|\n' 5 42
printf '%-*s|\n' 6 ab
printf '\x41\101\n'
printf 'a\0b' | od -c | head -1
printf '%s'
echo
printf '%d\n' 0x10 010
printf '%s=%s\n' a 1 b 2
printf '%5s|%-5s|\n' 'é' 'é'
printf '%d\n' abc 2>/dev/null; echo rc=$?
