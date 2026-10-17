class Grfsh < Formula
  desc "Small Unix shell with bash style scripting, typed tables and an agent mode"
  homepage "https://grf.sh"
  version "1.1.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.1.1/grfsh-v1.1.1-aarch64-apple-darwin.tar.gz"
      sha256 "ee34c4410d3946a049e7dbeb393921383fa38a32de67ba5f0f1b9e731599c24e"
    end
    on_intel do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.1.1/grfsh-v1.1.1-x86_64-apple-darwin.tar.gz"
      sha256 "5b3f98c56f31b9c0bdc445ed08a04075fea8cc4dec640721f46394730dad4c4c"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.1.1/grfsh-v1.1.1-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "523cb7c436d117cf4eb707b678ea81483ba1b100b8ec93058c2de05fa42c6726"
    end
    on_intel do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.1.1/grfsh-v1.1.1-x86_64-unknown-linux-musl.tar.gz"
      sha256 "a45a6d18fae1e17631010cb4d746b27727674c73783a5ce2e6e2b0bea15c987d"
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
