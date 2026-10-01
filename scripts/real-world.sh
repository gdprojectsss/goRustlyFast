#!/bin/sh
set -eu

grfsh="${GRFSH:-$PWD/target/release/grfsh}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

fetch() {
    curl -fsSL --retry 3 -o "$work/$1" "$2"
}

fetch rustup-init.sh https://sh.rustup.rs
fetch nvm.sh https://raw.githubusercontent.com/nvm-sh/nvm/master/nvm.sh
fetch ohmyzsh-install.sh https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh
fetch homebrew-install.sh https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh
fetch docker-install.sh https://get.docker.com
fetch neofetch https://raw.githubusercontent.com/dylanaraps/neofetch/master/neofetch

failed=0
for script in "$work"/*; do
    if "$grfsh" --check "$script" | grep ': error: '; then
        failed=1
    fi
done

mkdir -p "$work/home/.nvm"
cp "$work/nvm.sh" "$work/home/.nvm/nvm.sh"
expected="$(HOME="$work/home" NVM_DIR="$work/home/.nvm" bash -c '. "$NVM_DIR/nvm.sh"; nvm --version')"
actual="$(HOME="$work/home" NVM_DIR="$work/home/.nvm" "$grfsh" -c '. "$NVM_DIR/nvm.sh"; nvm --version')"
if [ "$expected" != "$actual" ]; then
    echo "nvm --version: bash printed '$expected', grfsh printed '$actual'"
    failed=1
fi

if [ "$failed" -ne 0 ]; then
    exit 1
fi
echo "all real-world scripts passed"
