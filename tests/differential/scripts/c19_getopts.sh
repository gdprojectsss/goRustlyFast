set -- -a -b val -x -c
while getopts ":ab:c" opt; do
  case $opt in
    a) echo A;; b) echo "B=$OPTARG";; c) echo C;;
    \?) echo "bad -$OPTARG";; :) echo "need arg -$OPTARG";;
  esac
done
shift $((OPTIND-1)); echo "rest=$# OPTIND=$OPTIND"
OPTIND=1; set -- -b; while getopts ":b:" o; do echo "o=$o arg=$OPTARG"; done
OPTIND=1; set -- -abfoo; while getopts "ab:" o; do echo "o=$o arg=${OPTARG-}"; done
