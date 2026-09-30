line="root:x:0:0::/root:/bin/bash"
IFS=: read -r user pass uid gid gecos home shell <<< "$line"
echo "$user $uid $home $shell [$gecos]"
data="a,b,,c"
IFS=,
set -- $data
echo $#
for f in "$@"; do printf '<%s>' "$f"; done; echo
unset IFS
words="  spaced   out  "
set -- $words
echo $#
