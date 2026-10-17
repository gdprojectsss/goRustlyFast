path=/usr/local/lib/file.tar.gz
echo "${path##*/} ${path%/*} ${path##*.} ${path%%.*} ${path%.*}"
f=file.tar.gz; echo "${f/.tar/}" "${f//[aeiou]/_}" "${f/#file/F}"
s=abcdef; echo "${s:2} ${s:2:2} ${s: -2} ${s:(-3):2} ${s::3}"
str="a-b-c"; echo "${str//-/ }"
pat='*'; echo "${f//$pat/X}" "${f//"$pat"/X}"
empty=""; echo "[${empty:-}][${empty:=filled}][$empty]"
set -- one two three; echo "${@:2}" "${*:1:2}" "${#}" "${@: -1}"
echo "${!#}"
arr=(a b c); echo "${!arr[@]}"
up=hello; echo "${up^}" "${up^^[lo]}"
echo "${#arr}" "${arr}"
k=1; echo "${arr[k]}" "${arr[k+1]}" "${arr[$k]}"
: ${zz:=default}; echo $zz
( : ${must:?is required} ) 2>&1 | sed 's/.*: //'
