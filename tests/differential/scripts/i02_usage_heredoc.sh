usage() {
  cat <<EOF2
Usage: $(basename "$0") [options]

Options:
  -h    help

EOF2
}
usage
cat > conf.ini <<EOF2
[a]
x=1

[b]
y=2
EOF2
grep -c '^$' conf.ini
