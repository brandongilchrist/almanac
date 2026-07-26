#!/usr/bin/env bash
# Boot the local SDLC: Gitea + Gitea Actions runner. Idempotent.
#
# Creates the admin user (almanac/almanac), the almanac/almanac repo,
# registers the act_runner against it, and adds a `local` git remote.
#
# After this completes:  sdlc/push.sh    (commit + push → triggers CI)
# Web UI:                http://localhost:3000   (almanac / almanac)

set -euo pipefail
cd "$(dirname "$0")"

GITEA=http://localhost:3000
ADMIN=almanac
PASS=almanac
REPO_OWNER=almanac
REPO_NAME=almanac

B=$'\033[1m'; G=$'\033[32m'; Y=$'\033[33m'; R=$'\033[31m'; N=$'\033[0m'
say() { printf "${B}${Y}▶ %s${N}\n" "$1"; }
ok()  { printf "${G}✓ %s${N}\n" "$1"; }
die() { printf "${R}✗ %s${N}\n" "$1"; exit 1; }

# Locate docker (Docker Desktop on macOS).
if ! command -v docker >/dev/null 2>&1; then
  if [ -x "/Applications/Docker.app/Contents/Resources/bin/docker" ]; then
    export PATH="/Applications/Docker.app/Contents/Resources/bin:$PATH"
  else
    die "docker not found. Install Docker Desktop or add docker to PATH."
  fi
fi

say "Starting Gitea + runner (docker compose up)"
docker compose up -d
ok "containers started"

say "Waiting for Gitea API to be healthy"
for i in $(seq 1 60); do
  if curl -fsS "$GITEA/api/v1/version" >/dev/null 2>&1; then
    ok "Gitea is up"; break
  fi
  sleep 2
  [ "$i" -eq 60 ] && die "Gitea did not become healthy in 120s"
done

say "Ensuring admin user '$ADMIN' exists"
# Create via CLI as the container's `git` user (gitea refuses to run as root).
# `user list` prints a table whose username is in the second whitespace column.
if ! docker exec -u git almanac-gitea gitea admin user list 2>/dev/null | awk 'NR>1{print $2}' | grep -qx "$ADMIN"; then
  docker exec -u git almanac-gitea gitea admin user create \
    --username "$ADMIN" --password "$PASS" --email dev@almanac.local \
    --admin --must-change-password=false >/dev/null 2>&1 || true
  ok "admin user created"
else
  # Ensure the password is the expected one (idempotent on re-runs).
  docker exec -u git almanac-gitea gitea admin user change-password \
    --username "$ADMIN" --password "$PASS" >/dev/null 2>&1 || true
  ok "admin user exists"
fi

say "Obtaining an API token"
TOKEN=$(curl -fsS -u "$ADMIN:$PASS" -X POST "$GITEA/api/v1/users/$ADMIN/tokens" \
  -H 'Content-Type: application/json' \
  -d '{"name":"bootstrap-'$(date +%s)'","scopes":["all"]}' \
  | grep -o '"sha1":"[^"]*"' | cut -d'"' -f4)
[ -n "$TOKEN" ] || die "could not obtain an API token"
ok "API token acquired"

say "Ensuring repo '$REPO_OWNER/$REPO_NAME' exists"
if curl -fsS -H "Authorization: token $TOKEN" "$GITEA/api/v1/repos/$REPO_OWNER/$REPO_NAME" >/dev/null 2>&1; then
  ok "repo already exists"
else
  curl -fsS -H "Authorization: token $TOKEN" -H 'Content-Type: application/json' \
    -X POST "$GITEA/api/v1/user/repos" \
    -d "{\"name\":\"$REPO_NAME\",\"private\":false,\"default_branch\":\"main\",\"description\":\"A calendar for agents and their artifacts.\"}" >/dev/null
  ok "repo created"
fi

say "Fetching runner registration token"
RUNNER_TOKEN=$(curl -fsS -H "Authorization: token $TOKEN" \
  "$GITEA/api/v1/repos/$REPO_OWNER/$REPO_NAME/actions/runners/registration-token" \
  | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
[ -n "$RUNNER_TOKEN" ] || die "could not fetch runner registration token (Actions enabled?)"

say "Registering / restarting the act_runner"
export RUNNER_TOKEN
docker compose up -d --force-recreate runner >/dev/null 2>&1
# Give the runner a moment to register + start the daemon.
sleep 4
if docker exec almanac-runner test -f /data/.runner 2>/dev/null; then
  ok "runner registered"
else
  printf "${Y}note:${N} runner still initializing — check http://localhost:3000/almanac/almanac/settings/actions/runners\n"
fi

# Add a `local` remote so push.sh works.
cd ..
if [ -d .git ]; then
  if ! git remote get-url local >/dev/null 2>&1; then
    git remote add local "http://$ADMIN:$PASS@localhost:3000/$REPO_OWNER/$REPO_NAME.git"
    ok "added git remote 'local'"
  fi
else
  printf "${Y}note:${N} no .git yet — run scripts/init-git.sh first, then push.sh\n"
fi

cat <<EOF

${B}${G}SDLC ready.${N}
  Gitea:      $GITEA  (user: $ADMIN / pass: $PASS)
  Repo:       $GITEA/$REPO_OWNER/$REPO_NAME
  Pipeline:   .gitea/workflows/ci.yml (fmt + clippy + test + build)

  Next: ${B}sdlc/push.sh${N}   # commit + push → triggers CI
  Runs: http://localhost:3000/$REPO_OWNER/$REPO_NAME/actions
EOF
