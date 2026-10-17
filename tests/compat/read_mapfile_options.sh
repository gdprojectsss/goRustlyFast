read <<< "  a  b  "
echo "[$REPLY]"
read -p 'not shown: ' prompted <<< "x"
echo "[$prompted]"
read -t 0 <<< "x"
echo "ready $?"
read -N 3 exact <<< "ab
cdef"
echo "[$exact] $?"
read -n 2 short <<< "éàü"
echo "[$short]"
read -N 2 wide <<< "éàü"
echo "[$wide]"
read -n 2 stop <<< "a
b"
echo "[$stop]"
read -u 7 fd7 7<<< "from fd 7"
echo "[$fd7]"
read -i ignored given <<< "given"
echo "[$given]"
read -N 4 partial <<< "ab"
echo "[$partial] $?"
read -N 3 -r raw <<< 'a\b'
echo "[$raw]"
read -N 3 cooked <<< 'a\bc'
echo "[$cooked]"
read -N 2 first second <<< "a b"
echo "[$first][$second]"
IFS=: read -N 4 one two <<< "x:yz"
echo "[$one][$two]"
read -a array -N 3 <<< "a b c"
echo "${#array[@]} [${array[0]}]"
read -n 3 -d , upto <<< "ab,cd"
echo "[$upto]"
read -n 2 escaped <<< '\ab'
echo "[$escaped]"
read -u 99 nothing 2>/dev/null
echo "bad fd $?"
read -u x nothing 2>/dev/null
echo "bad fd spec $?"
read -n -1 nothing 2>/dev/null
echo "bad count $?"
read -n 0 empty <<< "x"
echo "[$empty] $?"
read -N 3 spaces <<< "  a b"
echo "[$spaces]"
read -r -d '' nul < <(printf 'a\nb\0c')
echo "[$nul]"

printf 'a:b:c' | { mapfile -d : parts; echo "${#parts[@]} ${parts[0]} ${parts[2]}"; }
printf 'a\0b\0' | { mapfile -d '' -t nuls; echo "${#nuls[@]} ${nuls[0]} ${nuls[1]}"; }
printf '1\n2\n3\n' | { kept=(z z z z z); mapfile -O 2 -t kept; echo "${kept[*]}"; }
show() { echo "callback $1 [$2]"; }
printf 'l1\nl2\nl3\nl4\nl5\n' | { mapfile -t -C show -c 2 lines; echo "${lines[*]}"; }
printf 'a\nb\n' | { readarray -t -n 1 -s 1 picked; echo "${picked[*]}"; }
mapfile -t -u 9 fromfd 9<<< "fd nine"
echo "${fromfd[0]}"
mapfile -x 2>/dev/null
echo "bad option $?"
mapfile -c 0 -C show nothing < /dev/null 2>/dev/null
echo "bad quantum $?"
file=$(mktemp)
printf '1\n2\n3\n4\n' > "$file"
{ mapfile -n 2 -t head; read rest; } < "$file"
echo "${head[*]} then $rest"
{ read top; mapfile -t others; } < "$file"
echo "$top then ${others[*]}"
rm -f "$file"
