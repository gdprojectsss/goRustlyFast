#!/bin/sh
set -e
usage() { echo "usage: $0 [-h]" >&2; }
for arg do echo "arg: $arg"; done
x=${1:-none}; echo "x=$x"
case "$x" in a*) echo starts-a ;; *) echo other ;; esac
[ -n "$x" ] && [ "$x" != "b" ] && echo ok
test -d / -a -r / && echo dirtest
i=0; while [ $i -lt 3 ]; do i=`expr $i + 1`; done; echo i=$i
v=`echo \`echo nested\``; echo $v
echo "${#x}" ${x%?}
IFS=: read a b c <<EOT
1:2:3
EOT
echo "$a-$b-$c"
set -- p q r; shift; echo "$@" $#
echo "pid set: $(test -n "$$" && echo y)"
eval 'y=evaled'; echo $y
f() { echo "f args $# $1"; }; f "1 2" 3
printf '%s\n' done
