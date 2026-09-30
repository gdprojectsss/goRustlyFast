set -- -a -b value -cfile rest1 rest2
while getopts "ab:c:" opt; do
  echo "$opt=${OPTARG-none}"
done
shift $((OPTIND - 1))
echo "rest: $*"
