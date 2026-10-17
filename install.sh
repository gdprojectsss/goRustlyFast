#!/bin/sh
set -eu

site="${GRFSH_DOWNLOAD_BASE:-https://grf.sh}"
install_dir="${GRFSH_INSTALL_DIR:-$HOME/.local/bin}"
version="${GRFSH_VERSION:-latest}"

fail() {
    echo "grfsh install: $*" >&2
    exit 1
}

case "$(uname -s)/$(uname -m)" in
    Linux/x86_64) target=x86_64-unknown-linux-musl ;;
    Linux/aarch64 | Linux/arm64) target=aarch64-unknown-linux-gnu ;;
    Darwin/x86_64) target=x86_64-apple-darwin ;;
    Darwin/arm64) target=aarch64-apple-darwin ;;
    *) fail "no prebuilt binary for $(uname -s) $(uname -m), try: cargo install grfsh" ;;
esac

if [ "$version" = latest ]; then
    tag="$(curl -fsSL "$site/latest" | head -n 1)"
else
    tag="$version"
fi
[ -n "$tag" ] || fail "could not find a release"

name="grfsh-$tag-$target"
url="$site/download/$tag/$name.tar.gz"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -fsSL -o "$work/$name.tar.gz" "$url" || fail "download failed: $url"
curl -fsSL -o "$work/$name.tar.gz.sha256" "$url.sha256" || fail "checksum download failed"

expected="$(cut -d ' ' -f 1 "$work/$name.tar.gz.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$work/$name.tar.gz" | cut -d ' ' -f 1)"
else
    actual="$(shasum -a 256 "$work/$name.tar.gz" | cut -d ' ' -f 1)"
fi
[ "$expected" = "$actual" ] || fail "checksum mismatch for $name.tar.gz"

tar -xzf "$work/$name.tar.gz" -C "$work"
mkdir -p "$install_dir"
install -m 755 "$work/$name/grfsh" "$install_dir/grfsh"

echo "Installed grfsh $tag to $install_dir/grfsh"
echo "Run grfsh to try it. Your login shell stays the same until you change it."
case ":$PATH:" in
    *":$install_dir:"*) ;;
    *) echo "Add $install_dir to your PATH to run it by name." ;;
esac
echo "To make it your login shell:"
echo "  echo $install_dir/grfsh | sudo tee -a /etc/shells"
echo "  chsh -s $install_dir/grfsh"
