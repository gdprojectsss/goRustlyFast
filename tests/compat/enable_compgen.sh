enable -n echo
type -t echo
enable -n
enable echo
type -t echo
enable -n test
[ 1 = 1 ]
echo "bracket status $?"
enable test
enable no_such_builtin 2>/dev/null
echo "status $?"
enable | grep -c '^enable '
enable -s | head -3

f1() { :; }
f2() { :; }
foo=1
fob=2
shopt -s expand_aliases
alias fa=x
compgen -A function f
compgen -v fo
compgen -a f
compgen -b ec
compgen -k th
compgen -A keyword | wc -l
compgen -W "alpha beta alto" al
compgen -W "x y"
echo "status $?"
compgen -W "x y" z
echo "status $?"
compgen -W "ab ac b" -P "<" -S ">" a
compgen -W "ab ac b" -X "ac"
compgen -W "ab ac b" -X "!a*"
compgen -W "a b" -- a
compgen -A variable fob
compgen -A builtin -W extra exi
dir=$(mktemp -d)
mkdir "$dir/sub"
touch "$dir/file1"
compgen -f "$dir/fi" | sed "s|$dir|DIR|"
compgen -d "$dir/" | sed "s|$dir|DIR|"
compgen -G "$dir/*1" | sed "s|$dir|DIR|"
rm -rf "$dir"
