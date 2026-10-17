echo start
eval 'echo e1
fi
echo e2'
echo "eval status $?"
sourced=${TMPDIR:-/tmp}/grfsh_compat_syntax_$$.sh
printf 'echo s1\nif true; then echo BODY; elif\necho s2\n' > "$sourced"
. "$sourced"
echo "source status $?"
rm -f "$sourced"
if [[ -o noclobber ]]; then echo noclobber; else echo no noclobber; fi
echo before
if then echo BODY; else echo ELSE; fi
echo never
