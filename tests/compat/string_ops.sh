s="Hello, World"
echo ${#s} ${s:7} ${s:0:5} ${s,,} ${s^^}
echo ${s/World/There} ${s//l/L}
file=archive.tar.gz
echo ${file%%.*} ${file#*.} ${file%.gz}
empty=
echo "[${empty:-default}] [${empty-unused}] [${unset_var:=set_now}] [$unset_var]"
