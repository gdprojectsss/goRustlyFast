class Grfsh < Formula
  desc "Small Unix shell with bash style scripting and typed table pipelines"
  homepage "https://github.com/gdprojectsss/goRustlyFast"
  license "MIT"
  head "https://github.com/gdprojectsss/goRustlyFast.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
    man1.install "docs/grfsh.1"
  end

  test do
    assert_equal "4\n", shell_output("#{bin}/grfsh -c 'echo $((2 + 2))'")
    assert_match "grfsh", shell_output("#{bin}/grfsh --version")
  end
end
