#!/usr/bin/env bash
# Stop the local SDLC containers. Data (gitea-data/, runner-data/) persists
# on disk so the next `up.sh` resumes state.
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v docker >/dev/null 2>&1; then
  if [ -x "/Applications/Docker.app/Contents/Resources/bin/docker" ]; then
    export PATH="/Applications/Docker.app/Contents/Resources/bin:$PATH"
  fi
fi

docker compose down
echo "✓ SDLC stopped (data preserved in sdlc/gitea-data/)"
