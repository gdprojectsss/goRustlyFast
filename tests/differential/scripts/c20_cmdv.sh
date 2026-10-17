command -v ls >/dev/null 2>&1 || { echo missing; exit 1; }
command -v nonexistent_cmd_zz >/dev/null 2>&1 || { echo "missing zz"; }
type ls >/dev/null && echo typeok
hash nonexistent_zz 2>/dev/null || echo nohash
command -v cd; command -v echo
which -s ls 2>/dev/null; echo $?
