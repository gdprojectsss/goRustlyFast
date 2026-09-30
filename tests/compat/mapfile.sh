printf 'one\ntwo\nthree\n' > /tmp/grfsh_compat_mapfile.txt
mapfile -t lines < /tmp/grfsh_compat_mapfile.txt
echo ${#lines[@]} "${lines[2]}"
readarray -t -n 2 first < /tmp/grfsh_compat_mapfile.txt
echo "${first[@]}"
rm -f /tmp/grfsh_compat_mapfile.txt
