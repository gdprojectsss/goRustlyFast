declare -A color=([apple]=red [banana]=yellow)
color[cherry]=dark-red
for k in $(printf '%s\n' "${!color[@]}" | sort); do echo "$k=${color[$k]}"; done
echo ${#color[@]}
unset 'color[banana]'
echo ${#color[@]} ${color[apple]}
