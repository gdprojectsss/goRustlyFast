gen() {
  cat <<EOT
name: $1
  indented
EOT
}
x=$(gen foo); echo "$x"
y=$(cat <<'X'
literal $HOME
X
); echo "$y"
z=$(cat <<-T
	tabbed
	T
); echo "$z"
echo "$(echo "$(echo nested "quotes")")"
