#!/usr/bin/env bash
# Almanac local CI — the full SDLC quality gate, runnable without GitHub Actions.
#
# Mirrors what the Gitea Actions pipeline (sdlc/.gitea/workflows/ci.yml) runs.
# Exits non-zero on the first failing stage so output is easy to read.
#
# Usage:
#   scripts/ci.sh           # fmt + clippy + test + build
#   scripts/ci.sh quick     # fmt + clippy + unit tests only (no build)

set -euo pipefail

cd "$(dirname "$0")/.."

MODE="${1:-full}"
RED=$'\033[31m'; GREEN=$'\033[32m'; YELLOW=$'\033[33m'; BOLD=$'\033[1m'; RESET=$'\033[0m'

stage() { printf "\n${BOLD}${YELLOW}▶ %s${RESET}\n" "$1"; }
ok()    { printf "${GREEN}✓ %s${RESET}\n" "$1"; }
fail()  { printf "${RED}✗ %s${RESET}\n" "$1"; exit 1; }

stage "cargo fmt --check"
if cargo fmt --all --check; then ok "formatting clean"; else fail "formatting drift — run 'cargo fmt --all'"; fi

stage "cargo clippy (workspace, -D warnings)"
if cargo clippy --workspace --all-targets -- -D warnings; then ok "clippy clean"; else fail "clippy reported warnings"; fi

stage "cargo test (workspace)"
if cargo test --workspace; then ok "tests pass"; else fail "tests failed"; fi

if [ "$MODE" = "quick" ]; then
  printf "\n${GREEN}${BOLD}CI QUICK PASSED${RESET} (fmt + clippy + tests)\n"
  exit 0
fi

stage "cargo build --release (workspace)"
if cargo build --release --workspace; then ok "release build succeeds"; else fail "release build failed"; fi

stage "smoke: almanac demo produces valid ICS"
BIN=./target/release/almanac
if [ ! -x "$BIN" ]; then BIN=./target/debug/almanac; fi
"$BIN" demo > /tmp/almanac-ci-demo.ics
if "$BIN" validate /tmp/almanac-ci-demo.ics > /dev/null; then
  ok "demo feed is valid iCalendar"
else
  fail "demo feed failed validation"
fi

printf "\n${GREEN}${BOLD}CI PASSED${RESET} — fmt + clippy + tests + release build + smoke\n"
