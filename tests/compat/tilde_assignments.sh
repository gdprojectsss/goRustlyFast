check() { [ "$1" = "$2" ] && echo "$3 ok" || echo "$3 differs: $1"; }
path=~/a:~/b
check "$path" "$HOME/a:$HOME/b" "colon list"
path=x:~/b:~
check "$path" "x:$HOME/b:$HOME" "middle and end"
path=a=~/b
check "$path" "a=~/b" "second equals"
declare declared=~/d:~/e
check "$declared" "$HOME/d:$HOME/e" "declare"
export exported=~/x:~/y
check "$exported" "$HOME/x:$HOME/y" "export"
set -- a=~/x b:~/y
check "$1" "a=$HOME/x" "assignment-like argument"
check "$2" "b:~/y" "plain argument"
quoted="~/q:~/r"
check "$quoted" "~/q:~/r" "quoted"
root=~root
[ "$root" != "~root" ] && echo "user tilde expanded"
none=a:~nosuchuser_xyz/b
check "$none" "a:~nosuchuser_xyz/b" "unknown user"
