echo hi |& cat
ls /nonexistent_grfsh_dir |& wc -l | tr -d ' '
{ echo out; echo err >&2; } |& sort
file=${TMPDIR:-/tmp}/grfsh_compat_fd_$$
exec {fd}>"$file"
(( fd >= 10 )) && echo "allocated high"
echo hello >&"$fd"
echo world >&"$fd"
exec {fd}>&-
cat "$file"
exec {in}<"$file"
read -r first <&"$in"
echo "read $first"
exec {in}<&-
echo again {out}>"$file"
echo appended >&"$out"
exec {out}>&-
cat "$file"
rm -f "$file"
