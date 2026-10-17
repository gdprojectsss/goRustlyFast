echo $RANDOM >/dev/null; [[ $RANDOM =~ ^[0-9]+$ ]] && echo random
[[ $SECONDS =~ ^[0-9]+$ ]] && echo seconds
[[ $LINENO == 3 ]] && echo lineno
[[ -n $PPID ]] && echo ppid
[[ -n $PWD && -n $OLDPWD || -n $PWD ]] && echo pwd
[[ -n $SHLVL ]] && echo shlvl
[[ -n $BASHPID ]] && echo bashpid
[[ $- == *h* || 1 ]] && echo dash
echo "$_" >/dev/null
[[ -n $IFS ]] && echo ifs
[[ $PS4 == "+ " ]] && echo ps4
[[ -n $MACHTYPE ]] && echo machtype
[[ -n $HOSTTYPE ]] && echo hosttype
[[ -n $EPOCHSECONDS ]] && echo epoch
[[ -n $EPOCHREALTIME ]] && echo epochreal
[[ -n $BASH ]] && echo bashvar
[[ -n ${GROUPS[0]} ]] && echo groups
[[ -n $SRANDOM ]] && echo srandom
echo "n=${#BASH_ARGV[@]}" >/dev/null
