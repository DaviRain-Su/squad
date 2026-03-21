class Squad < Formula
  desc "Multi-AI-agent terminal collaboration daemon with MCP server and workflow engine"
  homepage "https://github.com/mco-org/squad"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/mco-org/squad/releases/download/v#{version}/squad-aarch64-apple-darwin.tar.gz"
      # sha256 is updated automatically by the Homebrew bump-formula-pr workflow
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end

    on_intel do
      url "https://github.com/mco-org/squad/releases/download/v#{version}/squad-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/mco-org/squad/releases/download/v#{version}/squad-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "squad"
    bin.install "squad-mcp"
    bin.install "squad-hook"
  end

  test do
    assert_match "Usage:", shell_output("#{bin}/squad help")
    assert_match "Usage:", shell_output("#{bin}/squad-hook help")
  end
end
