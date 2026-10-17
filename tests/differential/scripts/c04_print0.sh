mkdir -p d; touch "d/a b" "d/c
d" d/e
find d -type f -print0 | sort -z | while IFS= read -r -d '' f; do printf '<%s>\n' "$f"; done
find d -type f -print0 | xargs -0 ls -1 | sort | wc -l
cnt=0; while IFS= read -r -d '' f; do ((cnt++)) || true; done < <(find d -type f -print0); echo $cnt
