cat > .env <<'X'
# comment
FOO=bar
BAZ="qux quux"
export EXP=1
X
set -a; . ./.env; set +a
echo "$FOO|$BAZ|$EXP"
while IFS='=' read -r k v; do [[ $k =~ ^#|^$ ]] && continue; echo "key=$k val=$v"; done < .env
