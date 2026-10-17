case "$(uname -s)" in Darwin) os=mac;; Linux) os=linux;; *) os=other;; esac; echo $os
case "$OSTYPE" in darwin*) echo ostype-mac;; *) echo "ostype=[$OSTYPE]";; esac
arch=$(uname -m); echo ${arch/arm64/aarch64}
