nonexistent_cmd_xyz 2>/dev/null; echo $?
/etc/hosts 2>/dev/null; echo $?
bash -c 'exit 300'; echo $?
( exit -1 ) ; echo $?
sh -c 'kill -9 $$'; echo $?
f() { return 256; }; f; echo $?
exit_test() { exit 7; }; ( exit_test ); echo $?
