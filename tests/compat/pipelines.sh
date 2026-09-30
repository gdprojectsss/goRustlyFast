printf 'b\na\nc\n' | sort | head -2
echo hello | { read w; echo "got $w"; }
false | true; echo $?
true | false; echo $?
set -o pipefail
false | true; echo $?
! false; echo $?
