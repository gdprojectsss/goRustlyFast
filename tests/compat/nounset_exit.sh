local outside=1 2>/dev/null
echo "local outside a function: $?"
f() { local inside=2; echo "local inside: $inside"; }
f
set -u
value=$(echo "${undefined_one}" 2>/dev/null; echo still)
echo "substitution status $?"
(echo "$undefined_two") 2>/dev/null
echo "subshell status $?"
echo before
echo "$undefined_three"
echo never
