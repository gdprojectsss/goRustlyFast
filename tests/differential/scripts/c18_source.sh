here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$here/lib/helper.sh"
. "$(dirname "$0")/lib/helper.sh"
helper_fn
main() { helper_fn; }; main
echo "\$0 base: $(basename "$0")"
[[ "${BASH_SOURCE[0]}" == "$0" ]] && echo "run directly"
