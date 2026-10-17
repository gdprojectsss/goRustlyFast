coproc C { echo from-coproc; }
read -r line <&"${C[0]}"; echo "$line"
