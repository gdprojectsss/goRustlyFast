cat > s.sh <<'X'
echo in-s
return 5
echo not-here
X
source ./s.sh; echo "rc=$?"
cat > s2.sh <<'X'
echo "args: $# $1"
X
source ./s2.sh one two
set -- outer; source ./s2.sh; echo after
. s2.sh 2>/dev/null || echo "dot no-path failed"
