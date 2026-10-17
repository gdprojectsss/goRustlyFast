class Grfsh < Formula
  desc "Small Unix shell with bash style scripting, typed tables and an agent mode"
  homepage "https://grf.sh"
  version "1.0.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.0.2/grfsh-v1.0.2-aarch64-apple-darwin.tar.gz"
      sha256 "27ff5ef6c26c088ac7ff791827180e664cb5898ab82a27d3936261c0c4c1e5b6"
    end
    on_intel do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.0.2/grfsh-v1.0.2-x86_64-apple-darwin.tar.gz"
      sha256 "47acd76e213df063eb59eb3ef5f1f9393ffa576507d4d9462ece0e78bb89bb23"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.0.2/grfsh-v1.0.2-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "71f50eac1e0120072ab5b99663bda5cd4c97b20a7f8262b296228692f162c425"
    end
    on_intel do
      url "https://github.com/gdprojectsss/goRustlyFast/releases/download/v1.0.2/grfsh-v1.0.2-x86_64-unknown-linux-musl.tar.gz"
      sha256 "f43afe8d46f6b4d17d509cbe93d3f1b9dea8738c99f7a0fdef9aa2f696df6806"
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
