class Dtfls < Formula
  include Language::Python::Virtualenv

  desc "Portable dotfile sync from a git repo"
  homepage "https://github.com/wrnkt/dtfls"
  url "https://github.com/wrnkt/dtfls/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "REPLACE_WITH_SHA256_OF_THE_TAG_TARBALL"
  license "MIT"

  depends_on "python@3.12"

  def install
    virtualenv_install_with_resources
  end

  test do
    system "#{bin}/dtfls", "--help"
  end
end
