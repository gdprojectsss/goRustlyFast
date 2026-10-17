dir=$(mktemp -d)
mkdir "$dir/bin" "$dir/work"
echo 'echo "library from PATH: $*"' > "$dir/bin/lib.sh"
echo 'echo "library from cwd"' > "$dir/work/lib.sh"
echo 'echo "only in cwd"' > "$dir/work/local.sh"
cd "$dir/work"
PATH="$dir/bin:$PATH"
. lib.sh one two
source lib.sh
. local.sh
. ./lib.sh
shopt -u sourcepath
. lib.sh
shopt -s sourcepath
. missing_file.sh 2>/dev/null
echo "missing status $?"
. "$dir/bin" 2>/dev/null
echo "directory status $?"
source 2>/dev/null
echo "no argument status $?"
cd /
rm -rf "$dir"
