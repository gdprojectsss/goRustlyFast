second() { echo two; }
first() { echo one;echo b|cat; x=1 y=2 true >/dev/null 2>&1; }
branches() { if [ "$1" = a ]; then echo yes; elif [[ $1 == b* ]]; then echo b; else echo no; fi; }
loops() { for i in 1 "2 3" $x; do echo $i; done; for j; do :; done; while read -r line; do echo "$line"; done < /dev/null; until true; do break; done; }
cases() { case $1 in a|b) echo ab;; *) echo other;& x) ;;& esac; }
subshell() ( cd /tmp; ls )
mixed() { (( x++ )); for ((i=0;i<3;i++)); do echo $i; done; ! true && false || echo z; { echo g; } > /dev/null; (echo sub); }
docs() { local a=$(date) b=`pwd`; cat <<EOF
hello $a
EOF
echo after; cat <<-'X'
	literal
	X
}
quoting() { echo 'single' "double $x" \$escaped; arr=(1 2 "3 4"); echo ${arr[@]} >> /dev/null; }
function keyword { echo kw; }
redirected() { echo a; } 2>/dev/null
spaced() {
  # a comment
  echo one   two
  if true
  then
    echo nested
  fi
}
chained() { echo a && { echo b; echo c; } || while true; do break; done | cat; }
nested() { if true; then if false; then :; fi; fi; x=$((1+2)); [[ -n $x && $x != y ]] && echo k; }
declare -f
declare -F
declare -F first missing
echo "status $?"
declare -f missing
echo "status $?"
declare -f second
echo "status $?"
typeset -F second
source_text=$(declare -f docs)
unset -f docs
eval "$source_text"
docs | tail -2
type second
