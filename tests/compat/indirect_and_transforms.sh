target=value
ref=target
echo "${!ref}"
items=(alpha beta gamma)
pick="items[1]"
echo "${!pick}"
every="items[@]"
for item in "${!every}"; do echo "item $item"; done
echo "${!ref:-fallback} ${!ref^^}"
unset_ref=missing_variable
echo "${!unset_ref:-fallback}"
PREFIX_ONE=1
PREFIX_TWO=2
echo "${!PREFIX_*}"
for name in "${!PREFIX_@}"; do echo "name $name"; done
text="it's a test"
echo "${text@Q}"
empty=
echo "${empty@Q}"
escaped='tab\there'
echo "${escaped@E}"
echo "${text@U} ${text@u} ${text@L}"
declare -i number=4
declare -r fixed=1
echo "${number@a} ${fixed@a}"
