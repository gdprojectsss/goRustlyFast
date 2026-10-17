diff <(printf 'a\nb\n') <(printf 'a\nc\n'); echo rc=$?
echo hello > >(tr a-z A-Z)
sleep 0.2
tee >(wc -c > count) <<<"abcdef" >/dev/null; sleep 0.2; cat count
while read l; do echo "got $l"; done < <(echo x; echo y)
