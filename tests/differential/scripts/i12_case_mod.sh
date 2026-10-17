read -r ans <<< "Y"
case "${ans,,}" in y|yes) echo yes;; *) echo no;; esac
case $ans in [Yy]*) echo y2;; esac
