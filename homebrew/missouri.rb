# The release workflow renders this template into the tap on every tag:
# it fills the version and the four checksums from the build's own
# archives, then pushes the result to cjohnhanson/homebrew-tap. Edit
# the formula here, never in the tap.
class Missouri < Formula
  desc "Show-me-state: e2e testing as directed graphs of filesystem states"
  homepage "https://github.com/cjohnhanson/missouri"
  license "MIT"
  version "0.0.0"

  on_macos do
    on_arm do
      url "https://github.com/cjohnhanson/missouri/releases/download/v#{version}/missouri-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_DARWIN_ARM64"
    end
    on_intel do
      url "https://github.com/cjohnhanson/missouri/releases/download/v#{version}/missouri-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_DARWIN_X64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/cjohnhanson/missouri/releases/download/v#{version}/missouri-v#{version}-aarch64-unknown-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_LINUX_ARM64"
    end
    on_intel do
      url "https://github.com/cjohnhanson/missouri/releases/download/v#{version}/missouri-v#{version}-x86_64-unknown-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_LINUX_X64"
    end
  end

  def install
    bin.install "missouri"
    # Both names install. The short name exists because a registry name
    # was taken, and the typed name is missouri.
    bin.install_symlink bin/"missouri" => "msri"
    man1.install "man/missouri.1" if File.exist?("man/missouri.1")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/missouri --version")
  end
end
