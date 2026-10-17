( exit 3 ); echo $?
( false ); echo $?
x=$(exit 4); echo $?
if ! (exit 1); then echo neg; fi
set -e; (set -e; false; echo inner-continued) || echo "sub failed $?"
out=$(false) || echo "cmdsub failed"
f() { return 7; }; f || echo "f=$?"
{ false; } || echo grp
true | false; echo pipe=$?
set -o pipefail; true | false | true; echo pf=$?
false | true; echo pf2=$? PIPESTATUS=${PIPESTATUS[*]}
