# Local SDLC — Gitea + Gitea Actions

A self-contained software development lifecycle that runs entirely on your
machine via Docker. **Zero GitHub Actions minutes consumed.**

## What's here

```
sdlc/
├── docker-compose.yml      # Gitea (git server + web UI) + act_runner
├── runner-config.yaml      # act_runner config (capacity, cache, network)
├── up.sh                   # boot everything, create repo + admin + runner
├── push.sh                 # commit + push to local Gitea → triggers CI
├── down.sh                 # stop containers (data persists)
└── README.md               # this file
```

The CI pipeline itself lives at [`.gitea/workflows/ci.yml`](../.gitea/workflows/ci.yml)
and is an exact mirror of [`scripts/ci.sh`](../scripts/ci.sh):
**fmt → clippy → test → release build → smoke**.

## Quick start

```bash
sdlc/up.sh                            # boot Gitea + runner, create repo
sdlc/push.sh                          # push → CI triggers
open http://localhost:3000            # Gitea web UI (almanac / almanac)
# View runs: http://localhost:3000/almanac/almanac/actions
sdlc/down.sh                          # stop (data preserved in gitea-data/)
```

Default credentials: user `almanac`, password `almanac`. The script also
adds a `local` git remote so `git push local main` works directly.

## Three ways the quality gate runs

| Where | Command | GitHub minutes? |
|---|---|---|
| **Your shell** (fastest) | `scripts/ci.sh` | No |
| **Local Gitea Actions** | push → `sdlc/push.sh` | No |
| **GitHub Actions** (mirror) | push to `origin` | Yes (optional) |

All three run the identical stages. `scripts/ci.sh` is the primary local
gate; the Gitea pipeline proves the workflow definition is correct end-to-end
in a real CI runtime.

## Notes on the local runner

The `act_runner` provisions job containers via the host Docker socket. On
**Docker Desktop for macOS**, the first run after `up.sh` may need to pull
the `catthehacker/ubuntu:act-latest` job image (~3 GB) before jobs execute.
Pre-pull it once to avoid a timeout on the first run:

```bash
docker pull catthehacker/ubuntu:act-latest
```

If the runner accepts a task but no job container appears, restart it after
the image is cached:

```bash
docker compose -f sdlc/docker-compose.yml restart runner
```

The GitHub Actions mirror (`.github/workflows/ci.yml`) is the canonical
proof that the pipeline is valid; it runs the same stages and is kept in
sync with the Gitea workflow.
