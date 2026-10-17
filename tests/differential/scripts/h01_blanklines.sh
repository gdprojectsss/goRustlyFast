f() {
  echo "a

b"
}
f
echo "c

d"
g() { x="1

2"; echo "$x"; cat <<E
h1

h2
E
}; g
if true; then echo "e

f"; fi
