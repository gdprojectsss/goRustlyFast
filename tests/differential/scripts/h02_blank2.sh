cat <<E
x


y
E
printf "%s|" "$(printf "a\n\nb")"; echo
s="p

q"; echo ${#s}
