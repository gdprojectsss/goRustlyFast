name=world
cat <<-EOF
	indented $name
	EOF
cat <<"END"
raw $name `date`
END
for i in 1 2; do
cat <<EOF
loop $i
EOF
done
