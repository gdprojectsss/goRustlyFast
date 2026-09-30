echo out > /tmp/grfsh_compat_r.txt
echo more >> /tmp/grfsh_compat_r.txt
cat < /tmp/grfsh_compat_r.txt
ls /grfsh_missing_dir 2>/dev/null || echo "hidden error"
{ echo to-err >&2; } 2>&1 | tr a-z A-Z
cat <<EOF
heredoc $((1+1))
EOF
cat <<'EOF'
literal $HOME
EOF
tr a-z A-Z <<< "here string"
rm -f /tmp/grfsh_compat_r.txt
