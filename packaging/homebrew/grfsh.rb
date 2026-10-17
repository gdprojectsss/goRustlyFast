class Grfsh < Formula
  desc "Small Unix shell with bash style scripting, typed tables and an agent mode"
  homepage "https://grf.sh"
  version "1.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.1.0/grfsh-v1.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "41fcad63be4b9aa5a6f6d33bc671e2417a74bb19136139bbdc2c04890c5d0be3"
    end
    on_intel do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.1.0/grfsh-v1.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "09b80d1a0d38b3543059ec520656a945714875aebb40add5fbfeb37db34c692b"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.1.0/grfsh-v1.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "3d2b81b859d2abc14e7af469db3d9febb854190fe33a6cb596fb0e6e569eb5c7"
    end
    on_intel do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.1.0/grfsh-v1.1.0-x86_64-unknown-linux-musl.tar.gz"
      sha256 "a94c19514d019f88b268fdcd371899918d7fa00b606ae7486294ba6a9c1833ef"
    end
  end

  def install
    bin.install "grfsh"
    man1.install "grfsh.1"
  end

  def caveats
    <<~EOS
      To make grfsh your login shell:
        echo #{HOMEBREW_PREFIX}/bin/grfsh | sudo tee -a /etc/shells
        chsh -s #{HOMEBREW_PREFIX}/bin/grfsh
    EOS
  end

  test do
    assert_equal "42\n", shell_output("#{bin}/grfsh -c 'echo $((6 * 7))'")
    assert_match version.to_s, shell_output("#{bin}/grfsh --version")
  end
end
