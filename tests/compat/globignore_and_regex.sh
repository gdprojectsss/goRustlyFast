cd "$(mktemp -d)"
touch a.txt b.txt c.log .hidden
GLOBIGNORE='b*:*.log'
echo *
GLOBIGNORE='*'
shopt -s nullglob; echo "all ignored: [" * "]"; shopt -u nullglob
unset GLOBIGNORE
echo *
[[ "a b" =~ ^(a b)$ ]] && echo "paren space"
pat='a.c'; [[ abc =~ "$pat" ]] || echo "quoted literal"; [[ abc =~ $pat ]] && echo "unquoted regex"
[[ x =~ [[:alpha:]] ]] && echo class
v='*\*.txt'; echo $v
echo done
