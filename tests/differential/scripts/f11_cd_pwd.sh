mkdir -p real/sub; ln -s real link
cd link/sub; pwd; pwd -P | sed 's|.*/||'; echo "${PWD##*/}"
cd ..; pwd | sed 's|.*/||'
cd -P ../link && pwd | sed 's|.*/||'
cd /nonexistent 2>/dev/null || echo cdfail
CDPATH=/ cd usr >/dev/null && pwd
cd; [[ $PWD == $HOME ]] && echo home
