dir=$(mktemp -d)
cd "$dir"
mkdir -p g/sub/deep
touch g/A.TXT g/.hidden g/sub/deep/x.txt g/sub/y.txt

shopt -s failglob
echo *.nomatch
echo "same line"
echo "failglob status $?"
in_function() { echo *.nomatch; echo "inside"; }
in_function; echo "after the call"
substituted=$(echo *.nomatch; echo inside)
echo "[$substituted] $?"
shopt -u failglob

shopt -s nocasematch
[[ ABC == a* ]] && echo "pattern ignores case"
case HeLLo in hello) echo "case ignores case";; esac
[[ ABC =~ ^ab ]] && echo "regex ignores case"
shopt -u nocasematch
[[ ABC == a* ]] || echo "case matters again"

echo g/*.txt
shopt -s nocaseglob
echo g/*.txt
shopt -u nocaseglob
echo g/*
shopt -s dotglob
echo g/*
shopt -u dotglob
shopt -s globstar
echo g/**/*.txt
echo g/**
echo g/**/
shopt -u globstar
echo g/**/*.txt
shopt -s nullglob
echo g/*.none none
shopt -u nullglob

echo hi | read piped
echo "without lastpipe [$piped]"
shopt -s lastpipe
echo hi | read piped
echo "with lastpipe [$piped]"
count=0
printf 'a\nb\nc\n' | while read line; do count=$((count + 1)); done
echo "count $count"
false | true | (exit 3)
echo "${PIPESTATUS[*]} $?"
shopt -u lastpipe

cd /
rm -rf "$dir"
