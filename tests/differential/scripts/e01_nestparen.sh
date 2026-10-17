A=1 B=x
if [[ -n "$A" &&
      (("$B" == x && -n "$A") ||
      ("$B" == y)) ]]; then echo yes; else echo no; fi
[[ ( -n $A ) ]] && echo single
[[ ((-n $A)) ]] && echo double
