ver_ge() { [ "$(printf '%s\n' "$2" "$1" | sort -V | head -n1)" = "$2" ]; }
ver_ge 1.10.0 1.9.0 && echo ge
IFS=. read -r maj min pat <<< "3.12.1"; echo $((maj*10000+min*100+pat))
v=$(bash --version | head -1); [[ $v =~ version\ ([0-9]+) ]] && echo "bashmaj=${BASH_REMATCH[1]}" >/dev/null && echo ok
