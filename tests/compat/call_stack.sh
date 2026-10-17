here=${BASH_SOURCE[0]##*/}
lib=${TMPDIR:-/tmp}/grfsh_call_stack_$$.bash
names() {
    local sources="${BASH_SOURCE[*]##*/}"
    sources=${sources//grfsh_call_stack_$$.bash/LIB}
    echo "$1: ${#FUNCNAME[@]} [${FUNCNAME[*]}] [$sources] [${BASH_LINENO[*]}]"
}
echo "top: ${#FUNCNAME[@]} [${FUNCNAME[*]}] [${BASH_SOURCE[*]##*/}] [${BASH_LINENO[*]}] here=$here"
inner() {
    names inner
    echo "caller: ${FUNCNAME[1]} self: $FUNCNAME"
}
outer() {
    names outer
    inner
}
outer
printf 'lib() {\n  names lib\n}\nnames sourced\nlib\n' > "$lib"
. "$lib"
lib
wrapper() { . "$lib"; }
wrapper
rm -f "$lib"
