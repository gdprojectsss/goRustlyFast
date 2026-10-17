set -o pipefail
[[ -o pipefail ]] && echo on
[[ -o errexit ]] || echo off
[ -o pipefail ] && echo test-on
shopt -qo pipefail && echo shoptq
case $- in *e*) echo e;; *) echo no-e;; esac
