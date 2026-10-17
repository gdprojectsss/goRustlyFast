f() { getopts "a:" opt; echo "rc=$? opt=$opt OPTARG=${OPTARG-unset} OPTIND=$OPTIND"; }
OPTIND=1; f -a x
OPTIND=5; set -- -a; getopts "a" opt; echo "past end rc=$? OPTIND=$OPTIND OPTARG=${OPTARG-unset}"
OPTERR=0; OPTIND=1; getopts "b" opt -z; echo "quiet rc=$? opt=$opt"
for i in 1 2 3; do for j in a b; do break 5; done; echo never; done; echo "break capped"
for i in 1 2; do continue 9; echo never; done; echo "continue capped"
g() { break; }; for i in 1 2; do g; echo "after g $i"; done
(exit 300); echo "status $?"
h() { return 2 3; }; h 2>/dev/null; echo "return extra $?"
builtin -- ; echo "builtin dash $?"
builtin command echo "via builtin command"
shift 2>/dev/null x; echo "shift bad $?"
echo done
