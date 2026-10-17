x=val
cat <<EOF2
$x ${x} $(echo cmd) `echo bt` \$x \\ "q" 'sq'
EOF2
cat <<"EOF2"
$x literal
EOF2
cat <<'A' <<'B'
first
A
second
B
while read l; do echo "r:$l"; done <<END
l1
l2
END
f() { cat; } <<X
fromfunc
X
f
cat <<EOT | tr a-z A-Z
piped heredoc
EOT
cat << EOT
spaced delim
EOT
if true; then
	cat <<-EOT
	tab-stripped $x
	EOT
fi
