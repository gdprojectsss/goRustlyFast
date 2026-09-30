true; echo $?
false; echo $?
(exit 7); echo $?
nonexistent_command_xyz 2>/dev/null; echo $?
f() { return 42; }; f; echo $?
