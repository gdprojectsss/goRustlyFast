if [ 1 -lt 2 ]; then echo lt; fi
if [ "abc" = "abc" ] && [ -n "x" ]; then echo eq; fi
[ -z "" ] && echo empty
[[ hello == h*o ]] && echo glob
[[ hello == "h*o" ]] || echo quoted-literal
[[ abc < abd ]] && echo less
[[ -d /tmp && ! -f /tmp ]] && echo dir
if [[ v1.42.7 =~ ^v([0-9]+)\.([0-9]+) ]]; then echo "${BASH_REMATCH[1]} ${BASH_REMATCH[2]}"; fi
x=5
if (( x > 3 )) 2>/dev/null; then echo dparen; else echo nodparen; fi
