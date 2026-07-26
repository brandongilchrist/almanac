#!/usr/bin/env bash
# Commit the working tree (if dirty) and push to the local Gitea.
# Triggers the Gitea Actions CI pipeline.
set -euo pipefail
cd "$(dirname "$0")/.."

B=$'\033[1m'; G=$'\033[32m'; Y=$'\033[33m'; N=$'\033[0m'

if ! git remote get-url local >/dev/null 2>&1; then
  echo "${Y}No 'local' remote. Run sdlc/up.sh first.${N}"
  exit 1
fi

# Commit if there are staged or unstaged changes.
if [ -n "$(git status --porcelain)" ]; then
  git add -A
  git commit -m "chore: push to local Gitea ($(date -u +%FT%TZ))" >/dev/null
  echo "${G}✓ committed working tree${N}"
fi

echo "${B}▶ git push local${N}"
git push -u local HEAD:main || git push -f local HEAD:main
echo "${G}✓ pushed → CI triggered at http://localhost:3000/almanac/almanac/actions${N}"
