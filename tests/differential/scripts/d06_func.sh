function foo { echo "foo $1"; }
foo() { echo "redef $1"; }
foo x
bar() ( echo "subshell func"; exit 3 ); bar; echo rc=$?
baz() { return; }; baz; echo rc=$?
qux() { false; }; qux; echo rc=$?
declare -f baz >/dev/null && echo declared
declare -F | grep -c qux
unset -f qux; declare -F qux >/dev/null || echo gone
f() { echo "${FUNCNAME[0]} $#"; shift; echo "$@"; }; f 1 2 3
g() { local v=inner; h; }; h() { echo "dyn=$v"; }; v=outer; g; h
k() { echo "in k"; } >&2; k 2>/dev/null; echo kdone
export -f foo 2>/dev/null; bash -c 'foo exported' 2>/dev/null || echo "no export -f"
