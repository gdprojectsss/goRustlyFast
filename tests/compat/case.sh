for f in main.rs README.md Makefile notes.TXT "two words"; do
  case $f in
    *.rs) echo "$f rust";;
    *.md|*.txt) echo "$f text";;
    [A-Z]*file) echo "$f make";;
    "two words") echo "quoted";;
    *) echo "$f other";;
  esac
done
