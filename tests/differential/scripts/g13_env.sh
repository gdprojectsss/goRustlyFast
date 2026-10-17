[[ -n $HOME && -n $PATH && -n $USER ]] && echo base-env
env | grep -c '^BASH_' >/dev/null
echo "SHELL is set: ${SHELL:+yes}"
[[ $0 == *g13_env.sh ]] && echo dollar0
echo "$#:$1:$2"
echo "${@}"
echo $$ | grep -qE '^[0-9]+$' && echo pid
