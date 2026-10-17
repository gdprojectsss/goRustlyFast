set -e
x=0
((x++)) || true
echo x=$x
((x++))
echo x=$x
c=0; ((c++)); echo nope
