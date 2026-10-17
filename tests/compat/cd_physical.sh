base=$(mktemp -d)
real=$(cd -P "$base" && pwd)
mkdir "$base/target"
ln -s "$base/target" "$base/link"
cd "$base/link"
[ "$(pwd)" = "$base/link" ] && echo "logical pwd keeps the link"
[ "$(pwd -L)" = "$base/link" ] && echo "pwd -L keeps the link"
[ "$(pwd -P)" = "$real/target" ] && echo "pwd -P resolves the link"
cd ..
[ "$PWD" = "$base" ] && echo "cd .. is logical"
cd -P link
[ "$PWD" = "$real/target" ] && echo "cd -P sets a physical PWD"
cd -L "$base/link"
cd -P ..
[ "$PWD" = "$real" ] && echo "cd -P .. leaves the real parent"
cd -L "$base/link"
set -P
cd "$base/link"
[ "$PWD" = "$real/target" ] && echo "set -P makes cd physical"
[ "$(pwd)" = "$real/target" ] && echo "set -P makes pwd physical"
set +P
cd -x 2>/dev/null
echo "bad cd option status $?"
pwd -x 2>/dev/null
echo "bad pwd option status $?"
cd -e "$base/missing" 2>/dev/null
echo "missing directory status $?"
cd -- "$base"
[ "$PWD" = "$base" ] && echo "cd -- works"
cd /
rm -rf "$base"
