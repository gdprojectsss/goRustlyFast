umask 022
umask u=rwx,g=rx,o=
umask
umask -S
umask -p
umask -p -S
umask g+w
umask
umask o-r,a+x
umask
umask a=
umask
umask u=rwx,go=r
umask
umask +x
umask
umask ug=rwX
umask
umask u=g
umask
umask x=r 2>/dev/null
echo "status $?"
umask 999 2>/dev/null
echo "status $?"
umask -x 2>/dev/null
echo "status $?"
umask u=rq 2>/dev/null
echo "status $?"
umask 7
umask
umask 0022
umask go-w+x
umask
