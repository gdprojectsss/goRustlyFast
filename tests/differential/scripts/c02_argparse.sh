set -- --foo=bar -v --name "x y" -- rest1 rest2
verbose=0
while [[ $# -gt 0 ]]; do
  case $1 in
    --foo=*) foo=${1#*=}; shift;;
    -v|--verbose) verbose=1; shift;;
    --name) name=$2; shift 2;;
    --) shift; break;;
    -*) echo "unknown $1"; exit 2;;
    *) break;;
  esac
done
echo "foo=$foo v=$verbose name=$name rest=$*"
