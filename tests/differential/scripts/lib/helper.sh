echo "helper BASH_SOURCE=$(basename "${BASH_SOURCE[0]}") caller0=$(basename "${BASH_SOURCE[1]:-none}")"
helper_fn() { echo "in helper_fn ${FUNCNAME[0]} ${FUNCNAME[1]:-}"; }
