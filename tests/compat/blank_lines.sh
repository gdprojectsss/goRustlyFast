cat <<EOF
first

third


sixth
EOF
cat <<'RAW'
a

b
RAW
cat <<-TABS
	one

	two
	TABS
text="1

2"
echo "$text"
printf '%s\n' "x

y"
f() {

  echo in function

}
f
sourced=${TMPDIR:-/tmp}/grfsh_compat_blank_$$.sh
printf 'cat <<E\nsourced\n\nheredoc\nE\nv="p\n\nq"\necho "$v"\n' > "$sourced"
. "$sourced"
rm -f "$sourced"
