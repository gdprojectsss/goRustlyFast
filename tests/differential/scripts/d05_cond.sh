[[ -z "" && -n x ]] && echo 1
[[ 10 -gt 9 ]] && echo 2
[[ 10 > 9 ]] || echo 3-lexical
[[ abc < abd ]] && echo 4
[ "a" \< "b" ] && echo 5
[[ ! -e /nonexist ]] && echo 6
[[ -f /etc/hosts && -r /etc/hosts ]] && echo 7
[[ (1 -eq 1) && ( 2 -eq 3 || 3 -eq 3 ) ]] && echo 8
x=5; [[ $x -eq 5 ]] && echo 9
[[ "$x" == 5 ]] && echo 10
[ -t 0 ] || echo 11-not-tty
[[ 1 -eq 01 ]] && echo 12
[[ 08 -eq 8 ]] 2>/dev/null && echo 13 || echo 13-err
[[ -n $undefined ]] || echo 14
[[ a == a || $(echo side >&2) ]] && echo 15
test 2 -ge 1 -o 0 -eq 1 && echo 16
[ -e /etc/hosts ] && [ ! -d /etc/hosts ] && echo 17
[[ /a/b -ef /a/b ]] || echo 18
[[ -s /etc/hosts ]] && echo 19
[ "$x" = 5 ] && echo 20
[ -z ] && echo 21
[ ] || echo 22
[[ "abc" == a* ]] && echo 23
[[ x -nt /nonexist ]] || echo 24
