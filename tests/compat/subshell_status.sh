( exit 3 ); echo $?
( false ) || echo "subshell failed"
x=$(exit 5); echo $?
echo $( (exit 2); echo $? )
f() { ( return 4 ); echo "inside $?"; }; f
