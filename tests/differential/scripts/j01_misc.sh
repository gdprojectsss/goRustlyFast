source <(echo 'SRC=fromprocsub'); echo "$SRC"
eval "$(echo 'EV=1; export EV')"; echo $EV
eval "$(/opt/homebrew/bin/brew shellenv 2>/dev/null)"; [[ -n $HOMEBREW_PREFIX ]] && echo brewenv
shopt -s nocasematch; [[ ABC == abc ]] && echo nocase; shopt -u nocasematch
pat="a*"; [[ abc == $pat ]] && echo patvar; [[ abc == "$pat" ]] || echo patquoted
function g() { echo "fn with parens"; }; g
outer() { inner() { echo inner-def; }; inner; }; outer; inner
declare -g GLOB=1 2>/dev/null; f() { declare -g GV=2; }; f; echo "GV=$GV"
trap 'echo sigint' SIGINT; trap - SIGINT; echo sigprefix
type -P ls; command -V ls | head -1
echo $((16#ff)) $((36#z)) $((2#1010))
arr=(1 2 3); echo $(( ${#arr[@]} - 1 ))
while IFS=: read -r user _ uid _; do [[ $user == root ]] && echo "root uid=$uid"; done < /etc/passwd
x=$(cat <<< "$(echo inner)"); echo $x
cat <<< "${arr[@]}"
false; echo "PIPESTATUS=${PIPESTATUS[@]}"
printf '%s\n' "${arr[@]}" > out; wc -l < out | tr -d ' '
f2() { local x=$(false); echo "local rc=$?"; local y; y=$(false); echo "split rc=$?"; }; f2
alias e='echo aliased'; e hi
