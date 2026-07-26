#!/usr/bin/env bash
# Boot the local SDLC: Gitea + Gitea Actions runner. Creates the admin user,
# the `almanac/almanac` repo, and registers the runner — all idempotent.
#
# After this completes, run:  sdlc/push.sh
# Then open:                   http://localhost:3000  (almanac / almanac)

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

# Requires docker (Docker Desktop on macOS puts the CLI here if not on PATH).
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
# Login as admin (created on first boot via GITEA__admin__ env); if that fails,
# try to create via CLI inside the container.
TOKEN=$(curl -fsS -u "$ADMIN:$PASS" -X GET "$GITEA/api/v1/users/$ADMIN/token" \
  -H 'Content-Type: application/json' \
  -d '{"name":"bootstrap","scopes":["all"]}' 2>/dev/null | grep -o '"sha1":"[^"]*"' | cut -d'"' -f4 || true)

if [ -z "$TOKEN" ]; then
  say "Creating admin user via gitea CLI"
  docker exec almanac-gitea gitea admin user create \
    --username "$ADMIN" --password "$PASS" --email dev@almanac.local \
    --admin --must-change-password=false >/dev/null 2>&1 || true
  TOKEN=$(curl -fsS -u "$ADMIN:$PASS" -X GET "$GITEA/api/v1/users/$ADMIN/token" \
    -H 'Content-Type: application/json' \
    -d '{"name":"bootstrap","scopes":["all"]}' | grep -o '"sha1":"[^"]*"' | cut -d'"' -f4)
fi
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

say "Registering the act_runner"
RUNNER_TOKEN=$(curl -fsS -H "Authorization: token $TOKEN" \
  -X GET "$GITEA/api/v1/repos/$REPO_OWNER/$REPO_NAME/actions/runners/registration-token" \
  | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
[ -n "$RUNNER_TOKEN" ] || die "could not fetch runner registration token"

# Register (idempotent — skip if already registered).
if docker exec almanac-runner test -f /data/.registered 2>/dev/null; then
  ok "runner already registered"
else
  docker exec -e CONFIG_FILE=/config.yaml almanac-runner act_runner register --no-interactive \
    --instance http://gitea:3000 --token "$RUNNER_TOKEN" \
    --name almanac-runner --labels ubuntu-latest=docker://catthehacker/ubuntu:act-latest \
    >/dev/null 2>&1 || true
  docker exec almanac-runner touch /data/.registered 2>/dev/null || true
  docker compose restart runner >/dev/null 2>&1 || true
  ok "runner registered"
fi

# Set the local git remote so push.sh Just Works.
cd ..
if ! git remote get-url local >/dev/null 2>&1; then
  if [ -d .git ]; then
    git remote add local "http://$ADMIN:$PASS@localhost:3000/$REPO_OWNER/$REPO_NAME.git"
    ok "added git remote 'local'"
  else
    printf "${Y}note:${N} no .git yet — run scripts/init-git.sh first, then push.sh\n"
  fi
fi

cat <<EOF

${B}${G}SDLC ready.${N}
  Gitea:      $GITEA  (user: $ADMIN / pass: $PASS)
  Repo:       $GITEA/$REPO_OWNER/$REPO_NAME
  Pipeline:   .gitea/workflows/ci.yml (fmt + clippy + test + build)

  Next: ${B}sdlc/push.sh${N}   # commit + push → triggers CI
EOF
