RED='\033[0;31m'; NC='\033[0m'
echo -e "${RED}error${NC}" | od -An -c
printf "${RED}%s${NC}\n" err | od -An -c
printf '%b\n' "${RED}x" | od -An -c
echo $'\033[1m' | od -An -c
tput sgr0 >/dev/null 2>&1; echo tput=$?
