x="it's a b"
declare -i n=5
declare -a arr=(1 "two 2")
declare -A as=([k]=v)
echo "${x@Q}" "${x@A}" "${n@A}" "${n@a}" "${arr@a}" "${as@a}"
echo "${arr[@]@Q}"
echo "${arr[@]@A}"
echo "${as[@]@K}|"
echo "${as[@]@k}"
echo "${arr[@]@K}"
e='a\tb\x41'
echo "${e@E}"
up=hello
echo "${up@U}" "${up@u}" "${up@L}"
echo "${up^^[lo]}" "${up^[h]}" "${up^^l}" "${up,,}"
M=HELLO
echo "${M,,[LO]}" "${M,[H]}" "${M~~}" "${up~}" "${M~}"
words=(abc def)
echo "${words[@]^}" "${words[@]^^[ae]}"
unset un
echo "[${un@Q}]" "[${un@A}]" "[${un@a}]"
r=1
declare -r r
echo "${r@a}"
declare -ri ri=3
echo "${ri@A}"
declare -x ex=1
echo "${ex@a}" "${ex@A}"
emp=
echo "${emp@Q}" "${emp@A}"
tab=$'tab\there'
echo "${tab@Q}"
plain=plain
echo "${plain@K}" "${plain@k}"
declare -A only=([one]=1)
echo "${only[@]@A}"
echo "${only@A}"
prompt='\\ \[x\] \101'
echo "${prompt@P}"
