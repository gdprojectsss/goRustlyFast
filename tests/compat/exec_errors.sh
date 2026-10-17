directory=$(mktemp -d)

nopecmd 2>/dev/null
echo "not found $?"
nopecmd 2>&1 | grep -c "not found"
./nope 2>/dev/null
echo "missing path $?"
/etc 2>/dev/null
echo "directory $?"

printf 'echo "no shebang $# $1"\nexit 4\n' > "$directory/plain"
chmod +x "$directory/plain"
"$directory/plain" one
echo "plain script $?"

printf '\177ELF\000\001binary' > "$directory/binary"
chmod +x "$directory/binary"
"$directory/binary" 2>/dev/null
echo "binary $?"

printf 'echo text\n' > "$directory/unexecutable"
"$directory/unexecutable" 2>/dev/null
echo "not executable $?"

echo hi >&- 2>/dev/null
echo "closed stdout $?"
printf '%s\n' hi >&- 2>/dev/null
echo "closed printf $?"

exec 3>"$directory/three"
sh -c 'echo child writes >&3'
exec 3>&-
cat "$directory/three"

{ :; } 3>"$directory/scoped"
echo leaked >&3 2>/dev/null
echo "scoped fd $?"
cat "$directory/scoped"

read -r small <<< "here string"
echo "$small"
big=$(printf 'x%.0s' $(seq 6000))
read -r large <<< "$big"
echo "${#large}"
cat <<EOF | wc -l | tr -d ' '
one
two
EOF

set -e
value=$(false; echo y)
echo "substitution value=$value"
echo "[$(false; echo z)]"
set +e

exec -a renamed sh -c 'echo $0' &
wait

rm -rf "$directory"
