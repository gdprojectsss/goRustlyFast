trap 'echo exit trap' EXIT
trap 'echo err trap' ERR
false
echo after
