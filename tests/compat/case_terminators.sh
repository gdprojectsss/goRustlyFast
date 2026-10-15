classify() {
    case $1 in
        a*) echo "starts with a" ;&
        *b) echo "falls into b" ;;
        x*) echo "starts with x" ;;&
        *y) echo "ends with y" ;;&
        *) echo "always tested" ;;
    esac
}
classify apple
classify xy
classify zzz
case q in
    q) echo first ;&
    r) echo second ;&
    s) echo third ;;
    t) echo never ;;
esac
for value in 1 2; do
    case $value in
        1) echo "one"; continue ;&
        *) echo "other $value" ;;
    esac
done
