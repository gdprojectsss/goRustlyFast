export LATER
LATER=value
sh -c 'echo "LATER=$LATER"'
declare -x DECLARED
DECLARED=4
sh -c 'echo "DECLARED=$DECLARED"'
export GONE=5
export -n GONE
sh -c 'echo "GONE=${GONE-unset}"'
echo "still set: $GONE"
declare -p GONE
export PLUS=6
declare +x PLUS
sh -c 'echo "PLUS=${PLUS-unset}"'
export PENDING
declare -p PENDING
export -p | grep -E '^declare -x (LATER|DECLARED|PENDING)'
set -a
AUTO=1
set +a
MANUAL=2
sh -c 'echo "AUTO=${AUTO-unset} MANUAL=${MANUAL-unset}"'
set -o allexport
ALSO=3
set +o allexport
sh -c 'echo "ALSO=${ALSO-unset}"'
case $- in *a*) echo "a in flags" ;; *) echo "a not in flags" ;; esac
set -a
case $- in *a*) echo "a in flags" ;; esac
set +a
inner() {
    local HOME=/elsewhere
    sh -c 'echo "child sees local HOME=$HOME"'
}
inner
[ "$HOME" != /elsewhere ] && echo "HOME restored"
scoped() {
    local LOCAL_ONLY=1
    export LOCAL_ONLY
    sh -c 'echo "LOCAL_ONLY=$LOCAL_ONLY"'
}
scoped
sh -c 'echo "after function LOCAL_ONLY=${LOCAL_ONLY-unset}"'
list=(1 2)
export list
echo "export array status $?"
declare -p list
readonly LOCKED=1
export LOCKED=2
echo "export readonly status $?"
