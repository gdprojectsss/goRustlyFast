file="my file.txt"; touch "$file"
ls "$file" >/dev/null && echo quoted-ok
files=$(ls *.txt); echo "$files"
for f in *.txt; do echo "<$f>"; done
cmd=(ls -1 "$file"); "${cmd[@]}"
eval "ls \"\$file\"" 
echo "$(basename "$(pwd)")" | grep -q wd && echo nested-q
echo "${file// /_}"
