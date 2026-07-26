# Homebrew formula for Almanac.
#
# Install with:
#   brew tap brandongilchrist/almanac https://github.com/brandongilchrist/almanac
#   brew install almanac
#
# Or one-line:
#   brew install brandongilchrist/almanac/almanac
#
# Provides: `almanac` (CLI), `almanac-server`, `almanac-mcp`.

class Almanac < Formula
  desc "Agent-native calendar — iCalendar feeds with artifact-lineage checkmarks"
  homepage "https://github.com/brandongilchrist/almanac"
  url "https://github.com/brandongilchrist/almanac/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license "MIT"
  head "https://github.com/brandongilchrist/almanac.git", branch: "main"

  # Builds from source via cargo. Prebuilt bottles can be added to the release
  # for faster installs once the tarball sha256 is filled in above.
  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--workspace"
    # Install the three binaries.
    bin.install "target/release/almanac"
    bin.install "target/release/almanac-server"
    bin.install "target/release/almanac-mcp"
  end

  test do
    assert_match "Almanac", shell_output("#{bin}/almanac --version 2>&1", 0)
    # demo feed renders valid ICS.
    ics = shell_output("#{bin}/almanac demo")
    assert ics.include?("BEGIN:VCALENDAR")
    assert ics.include?("UID:daily-brief@almanac")
  end
end
