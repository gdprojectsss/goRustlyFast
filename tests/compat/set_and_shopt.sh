set -o
set +o | head -5
echo "flags $-"
set -fC
echo "flags $-"
set +fC
set -o no_such_option 2>/dev/null
echo "bad name status $?"
set -q 2>/dev/null
echo "bad letter status $?"
set -o errexit -o nounset
echo "flags $-"
set +eu
set -x -
echo "dash clears xtrace: $-"
set - one two
echo "$# $1 $2"
set -o vi
set -o | grep -E '^(vi|emacs) '
set -o emacs
set -o | grep -E '^(vi|emacs) '
set +o emacs

dir=$(mktemp -d)
cd "$dir"
touch one.txt two.txt
set -f
echo *.txt
set +f
set -o noglob
echo *.txt
set +o noglob
echo *.txt

echo first > file
set -C
echo second > file 2>/dev/null
echo "clobber status $?"
cat file
echo third >| file
cat file
echo more >> file
cat file
echo nothing > /dev/null
echo "device status $?"
set +C
echo fourth > file
cat file
cd /
rm -rf "$dir"

shopt
shopt -p | head -3
shopt -s | head -3
shopt -u | head -3
shopt -q dotglob
echo "dotglob status $?"
shopt -qo errexit
echo "errexit status $?"
shopt -so pipefail
echo "flags $-"
set -o | grep pipefail
shopt -uo pipefail
shopt -po errexit pipefail
shopt -o -p | head -2
shopt -s nocasematch extglob checkwinsize
shopt -p nocasematch extglob checkwinsize
shopt nocasematch sourcepath
shopt dotglob nullglob
echo "listing status $?"
shopt no_such_option 2>/dev/null
echo "bad option status $?"
shopt -s -u dotglob 2>/dev/null
echo "set and unset status $?"
shopt -x 2>/dev/null
echo "bad flag status $?"
set -o posix
echo "posix status $?"
set +o posix
