greet() { echo "hello $1"; }
type -t if greet echo cd
type -t no_such_command_here
echo "status $?"
type if
type greet
type -t [[ ]] { } ! function time case
type -p echo
echo "path only status $?"
type -f greet 2>/dev/null
echo "skip functions status $?"
type no_such_command_here 2>/dev/null
echo "missing status $?"
type -x echo 2>/dev/null
echo "bad option status $?"
type
echo "no names status $?"
command -v greet echo if
command -v no_such_command_here
echo "command -v missing status $?"
command -v no_such_command_here greet
echo "command -v mixed status $?"
command -V greet echo if
command -V no_such_command_here 2>/dev/null
echo "command -V missing status $?"
command -v
echo "command -v empty status $?"
command
echo "bare command status $?"
command greet world 2>/dev/null
echo "command skips functions status $?"
echo text | command -p cat
command -p -- printf '%s\n' "default path"
shopt -s expand_aliases
alias ll='ls -l'
type ll
type -t ll
command -v ll
command -V ll
