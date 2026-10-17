set -o | grep -E '^(errexit|nounset|pipefail)' | awk '{print $1,$2}'
set -o errexit; [[ $- == *e* ]] && echo has-e; set +o errexit
set -u; f() { echo "${1:-noarg}"; }; f; set +u
set -o posix 2>/dev/null; echo posix=$?; set +o posix 2>/dev/null
set -C 2>/dev/null; echo C=$?
set -n 2>/dev/null; echo "n doesn't stop in bash? printed"
