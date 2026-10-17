export PATH="/opt/example/bin:$PATH"
case $PATH in /opt/example/bin:*/bin*) echo "path kept" ;; *) echo "path lost: [$PATH]" ;; esac
export PATH
case $PATH in /opt/example/bin:*) echo "still kept" ;; *) echo "lost on export" ;; esac
export HOME
[ -n "$HOME" ] && echo "home kept"
export NEVER_SET_BEFORE
env | grep -c '^NEVER_SET_BEFORE=' || true
NEVER_SET_BEFORE=now
export LATER=value
sh -c 'echo "child sees $LATER"'
