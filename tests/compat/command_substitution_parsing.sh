echo $( case x in x) echo matched;; esac )
echo "$( case y in (y) echo quoted;; esac )"
kind() {
  echo "$(case $1 in a|b) echo letter;; [0-9]) echo digit;; *) echo other;; esac)"
}
kind a
kind 7
kind %
x=$(echo a # comment with )
)
echo "[$x]"
echo $(echo ")"; echo "(")
echo $((echo a); (echo b))
((echo nested subshell) )
echo $(( 1 + (2 * 3) ))
echo $(echo $(case z in z) echo inner;; esac))
y=$(cat <<EOF
heredoc in substitution
EOF
)
echo "$y"
echo `echo backtick`
