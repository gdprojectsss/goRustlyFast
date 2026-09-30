mkdir -p /tmp/grfsh_compat_glob && cd /tmp/grfsh_compat_glob && touch b.txt a.txt c.md .hidden
echo *.txt
echo *.nomatch
echo ?.md
echo [ab].txt
echo '*.txt'
cd / && rm -rf /tmp/grfsh_compat_glob
