#!/usr/bin/env bash
# Almanac one-line installer:  curl -fsSL https://almanac.dev/install.sh | bash
#
# Installs the `almanac` CLI + the `almanac-mcp` MCP server + the `almanac-server`
# HTTP server for the current platform. Downloads the right prebuilt binary from
# GitHub Releases. Falls back to building from source via cargo if no prebuilt
# binary exists for the platform.
#
# No terminal required for the desktop app itself — that's the .dmg/.msi/.AppImage
# from the Releases page. This script is for the CLI/server/MCP tooling.

set -euo pipefail

OWNER="brandongilchrist"
REPO="almanac"
VERSION="${ALMANAC_VERSION:-latest}"

B=$'\033[1m'; G=$'\033[32m'; Y=$'\033[33m'; R=$'\033[31m'; N=$'\033[0m'
say() { printf "${B}▶ %s${N}\n" "$1"; }
ok()  { printf "${G}✓ %s${N}\n" "$1"; }
die() { printf "${R}✗ %s${N}\n" "$1"; exit 1; }

# ---- detect platform ----
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Darwin) PLATFORM="apple-darwin";;
  Linux)  PLATFORM="unknown-linux-gnu";;
  *) die "Unsupported OS: $OS. Use cargo install (see README).";;
esac
case "$ARCH" in
  arm64|aarch64) ARCH_NORM="aarch64";;
  x86_64|amd64)  ARCH_NORM="x86_64";;
  *) die "Unsupported arch: $ARCH";;
esac
TARGET="${ARCH_NORM}-${PLATFORM}"
say "Detected platform: $TARGET"

# ---- install dir ----
INSTALL_DIR="${ALMANAC_INSTALL_DIR:-${HOME}/.almanac/bin}"
mkdir -p "$INSTALL_DIR"

# ---- resolve version + asset URL ----
if command -v curl >/dev/null 2>&1; then
  FETCH="curl -fsSL"
elif command -v wget >/dev/null 2>&1; then
  FETCH="wget -qO-"
else
  die "Need curl or wget."
fi

if [ "$VERSION" = "latest" ]; then
  say "Resolving latest release"
  VERSION="$($FETCH "https://api.github.com/repos/$OWNER/$REPO/releases/latest" \
    | grep -m1 '"tag_name"' | cut -d'"' -f4 | sed 's/^v//')"
  [ -n "$VERSION" ] || die "could not resolve latest version"
fi
ok "version $VERSION"

ASSET="almanac-${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/$OWNER/$REPO/releases/download/v${VERSION}/${ASSET}"

# ---- download prebuilt, or fall back to cargo build ----
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if $FETCH "$URL" -o "$TMP/$ASSET" 2>/dev/null && [ -s "$TMP/$ASSET" ]; then
  say "Downloading prebuilt binaries"
  tar -xzf "$TMP/$ASSET" -C "$TMP"
  mv "$TMP"/almanac* "$INSTALL_DIR/" 2>/dev/null || true
  ok "extracted to $INSTALL_DIR"
else
  say "${Y}No prebuilt binary for $TARGET — building from source${N}"
  if ! command -v cargo >/dev/null 2>&1; then
    die "Rust (cargo) is required to build from source. Install from https://rustup.rs"
  fi
  say "cargo install almanac-cli (this takes a few minutes)"
  cargo install --git "https://github.com/$OWNER/$REPO" almanac-cli almanac-server almanac-mcp --root "$INSTALL_DIR" --locked
  ok "built and installed to $INSTALL_DIR"
fi

# ---- path hint ----
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    printf "${B}Add Almanac to your PATH:${N}\n"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    # Append to shell rc if not already there.
    RC=""
    [ -n "${ZDOTDIR:-}" ] && RC="$ZDOTDIR/.zshrc" || RC="$HOME/.zshrc"
    [ -f "$HOME/.bashrc" ] && [ -z "${ZSH_VERSION:-}" ] && RC="$HOME/.bashrc"
    if [ -n "$RC" ] && ! grep -q "$INSTALL_DIR" "$RC" 2>/dev/null; then
      echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$RC"
      ok "added to $RC"
    fi
    ;;
esac

echo ""
printf "${B}${G}Almanac ${VERSION} installed.${N}\n"
echo "  Run the server:   almanac serve"
echo "  Open the app:     open http://localhost:8787"
echo "  MCP for agents:   point your MCP client at: almanac-mcp"
echo "  Docs:             https://github.com/$OWNER/$REPO#readme"
