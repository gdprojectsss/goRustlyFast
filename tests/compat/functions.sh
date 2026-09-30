greet() { echo "hi $1"; return 3; }
greet world; echo "status $?"
outer() { local v=inner; inner_fn; echo "outer sees $v"; }
inner_fn() { echo "inner sees $v"; v=changed; }
v=global
outer
echo "global is $v"
recurse() { [ $1 -le 0 ] && return; echo $1; recurse $(( $1 - 1 )); }
recurse 3
function kw { echo "keyword form"; }
kw
