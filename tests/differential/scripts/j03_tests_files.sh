touch f; chmod 644 f; ln -sf f l; mkdir -p d
[[ -L l ]] && echo symlink
[[ -h l ]] && echo h
[[ -x f ]] || echo notexec
[[ -w f ]] && echo writable
[[ -O f ]] && echo owned
[[ -G f ]] && echo group
[[ -p f ]] || echo notpipe
[[ -S f ]] || echo notsock
[[ -b f || -c /dev/null ]] && echo chardev
[[ f -nt /etc/hosts || 1 ]] && echo nt
[[ -k /tmp ]] && echo sticky
[[ -u /usr/bin/sudo ]] && echo suid
[[ -e d/ && -d d ]] && echo dir
[ -a f ] && echo dash-a
test -e f -a -f f && echo testa
