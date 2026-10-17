if [[ -n x && (("a" == a) || ("b" == c)) ]]; then
  echo guarded
else
  echo "else ran"
fi
echo end2
