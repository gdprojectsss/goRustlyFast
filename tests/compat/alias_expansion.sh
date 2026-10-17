shopt -s expand_aliases
alias a=b
alias b='echo B'
alias e='echo '
e a
alias n=
n
echo status=$?
alias hi='echo hello world '
alias punct='!!!'
hi punct
alias one='ONE ' two='TWO ' three='THREE'
e one \
  two one \
  two three two \
  one
alias e_='for i in 1 2 3; do echo $i;'
e_ done
alias FOR1='for ' FOR2='FOR1 ' eye1='i ' eye2='eye1 ' IN='in '
FOR2 eye2 IN 4 5; do echo $i; done
alias LEFT='{'
LEFT echo left; echo brace; }
alias PAREN='('
PAREN echo left; echo paren )
alias c='cat <<EOF
here $((1 + 2))
EOF
'
c
alias echo-x='echo $x '
x=X
echo-x echo-x
alias -- q="it's"
alias q
alias -p | grep q=
alias nope q; echo status=$?
alias -z; echo status=$?
alias 'a/b=c'; echo status=$?
unalias; echo status=$?
unalias q nope; echo status=$?
unalias -a
alias; echo status=$?
