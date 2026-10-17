[[ -o errexit ]]; echo "errexit off: $?"
[ -o errexit ]; echo "test errexit off: $?"
[[ -o nounset ]] || echo "nounset off"
set -u
[[ -o nounset ]] && echo "nounset on"
[ -o nounset ] && echo "test nounset on"
set +u
set -o pipefail
[[ -o pipefail ]] && echo "pipefail on"
[[ -o no_such_option ]]; echo "unknown: $?"
[[ ! -o errexit ]] && echo negated
a=1
[[ $a == 1 && (( $a == 1 ) || ( $a == 2 )) ]] && echo nested
[[ ( $a == 2 ) || ( ( $a == 1 ) && -n x ) ]] && echo grouped
! ! true; echo "double negation: $?"
! ! false; echo "double negation false: $?"
! true; echo "single negation: $?"
