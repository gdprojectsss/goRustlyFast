alias greet='echo hello'
greet 2>/dev/null || echo "not expanded in scripts"
shopt -q expand_aliases || echo "expand_aliases is off"
alias ls 2>/dev/null || echo "no default ls alias"
shopt -s expand_aliases
greet
shopt -q expand_aliases && echo "expand_aliases is on"
f() { greet from function; }
f
shopt -u expand_aliases
greet 2>/dev/null || echo "off again"
echo "${BASH_VERSION+set}" > /dev/null
echo "$PS4"
case $- in *i*) echo interactive ;; *) echo not interactive ;; esac
[[ $SHLVL =~ ^[0-9]+$ ]] && echo "shell level"
