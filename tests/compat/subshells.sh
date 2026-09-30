x=1
(x=2; echo "inner $x")
echo "outer $x"
echo "$(echo nested $(echo deep))"
y=$(printf 'a\n\n\n'); echo "[$y]"
{ echo group; echo two; } | wc -l | tr -d ' '
