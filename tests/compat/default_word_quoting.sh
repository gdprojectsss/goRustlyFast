unset u
echo "${u:-'single'}" "${u:-\"dq\"}" "${u-a\}b}"
printf '<%s>' ${u:-"a b" c}; echo
set -- ax b; IFS=x; printf '<%s>' $@; echo; IFS=' '
x='a  b'; printf '<%s>' ${#x}; echo
arr=(zero one); ref=arr; echo "${!ref}"
a=(p q); i=1; n='a[i]'; echo "${!n}"
s=hello; echo "${s////c}" "${s//l/L}"
echo ~ | grep -q / && echo tilde
v=x; w=${u-~}; [ "$w" = "$HOME" ] && echo "tilde default"
echo done
