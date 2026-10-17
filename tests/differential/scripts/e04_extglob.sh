shopt -s extglob
touch a.txt b.md c.log
echo !(*.txt)
echo @(a|b).*
x="  trim  "; echo "[${x##+([[:space:]])}]"
v=abc123; echo "${v//+([0-9])/N}"
case foo.md in *.@(md|txt)) echo ext;; esac
[[ abc == +([a-c]) ]] && echo cond
