tmp=$(mktemp -d)
trap 'rm -rf "$tmp"; echo cleaned' EXIT INT TERM
touch "$tmp/x"
( kill -TERM $$ ) 
sleep 0.2
echo after
