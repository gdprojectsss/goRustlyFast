ls() { echo "fn ls"; }
ls
command ls -d / 
cd() { echo "wrapped cd"; command cd "$@"; }; cd /
echo() { printf 'E:%s\n' "$*"; }; echo hi; command echo plain; unset -f echo; echo restored
