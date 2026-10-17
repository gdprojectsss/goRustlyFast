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
case "${SHELL:-}" in
    "" | */grfsh)
        if [ "$(uname -s)" = Darwin ]; then current_shell=/bin/zsh; else current_shell=/bin/bash; fi
        ;;
    *) current_shell="$SHELL" ;;
esac
echo
echo "Try it first without switching:"
echo "  $install_dir/grfsh -l"
echo "  $install_dir/grfsh --check ~/.zshrc    # shows which lines of a zsh or bash file grfsh cannot run"
echo "Type exit to get back to your current shell."
echo
echo "To make it your login shell:"
echo "  grep -qx $install_dir/grfsh /etc/shells || echo $install_dir/grfsh | sudo tee -a /etc/shells"
echo "  chsh -s $install_dir/grfsh"
echo "Keep a terminal with your current shell open until a new one starts grfsh correctly."
echo
echo "To switch back:"
echo "  chsh -s $current_shell"
if [ "$(uname -s)" = Darwin ]; then
    echo "  Terminal: Settings > General > Shells open with: Default login shell"
    echo "  iTerm2: Settings > Profiles > General > Command: Login Shell"
fi
echo "  Ghostty: remove the command = line from its config file"
echo "If a startup file breaks grfsh, start it with grfsh -l --noprofile --norc and fix the file."
echo "More: https://github.com/gdprojectsss/goRustlyFast/blob/main/docs/switching-back.md"
