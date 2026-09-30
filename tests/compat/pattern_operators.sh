path=/usr/local/lib/libfoo.so.1.2
echo ${path##*/} ${path#*/} ${path%.*} ${path%%.*}
echo ${path/lib/LIB} ${path//lib/LIB} ${path/#\/usr/USR} ${path/%2/two}
name=hello
echo ${name^} ${name^^} ${name,} X${name^^}X
up=WORLD
echo ${up,,} ${up,}
