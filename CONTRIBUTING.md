# Contributing to Almanac

Almanac is a small, tightly-scoped project: a calendar + artifact-lineage
rendering layer. The quickest way to get merged is to keep changes inside
that scope.

## The contract

Before any change, re-read the **"What Almanac is not"** section of
[`docs/00_OVERVIEW.md`](docs/00_OVERVIEW.md). If a change requires building
a web UI, replacing the cron engine, running agents, or inventing a new
protocol, it is out of scope — file an issue first.

## Local development

```bash
# One-time: build everything.
cargo build

# The full quality gate (fmt + clippy + tests + release build + smoke).
scripts/ci.sh

# Or the fast path while iterating (no build).
scripts/ci.sh quick
```

Requirements:
- Rust 1.75+ (tested on 1.83 stable and 1.93 nightly).
- For the website/demo: any browser. No build step.

## Local SDLC (Gitea + Gitea Actions, no GitHub minutes)

The repo ships a self-contained CI that runs on your machine via Docker:

```bash
sdlc/up.sh       # boots Gitea + runner, creates the repo, registers the runner
sdlc/push.sh     # commit + push → CI runs the same pipeline as ci.sh
open http://localhost:3000   # Gitea web UI (almanac / almanac)
sdlc/down.sh     # stop (data persists)
```

The pipeline (`.gitea/workflows/ci.yml`) mirrors `scripts/ci.sh` exactly:
fmt → clippy → test → release build → demo smoke.

## Coding conventions

- **No `unwrap()`/`expect()` in production paths.** Use `?` and proper error
  types (`IngestError`, `IcalError`, `LineageError`).
- **One commit per concern.** Match the existing message style:
  `feat(almanac): …`, `fix(almanac): …`, `test(almanac): …`, `docs(almanac): …`.
- **Doc comments on public API.** Module-level docs explain the *why*.
- **Tests are mandatory.** Every new rendering rule, ingestor, or lineage
  path needs a unit test. Feed changes need an assertion that the output
  still parses as valid iCalendar.
- **The plan is the source of truth.** Kinds (48050–48054), the ICS field
  mapping, and the data-model decisions live in [`docs/10_PLAN.md`](docs/10_PLAN.md).
  If reality diverges from the plan, update the plan first, then the code.

## ICS conformance

Before merging any feed change, confirm the output validates:

```bash
cargo run -p almanac-cli -- demo > /tmp/feed.ics
cargo run -p almanac-cli -- validate /tmp/feed.ics
# or paste /tmp/feed.ics into https://icalendar.org/validator.html
```

Most "works in Google, breaks in Apple" bugs are spec-conformance issues.

## Branching

- `main` is always green.
- Branch per feature/fix: `feat/<thing>`, `fix/<thing>`, `docs/<thing>`.
- Squash-merge PRs; keep the goal-by-goal history when it adds clarity.

## Reporting issues

- **Bug in rendering?** Include the raw ICS output and which client it
  broke in.
- **Lineage verdict wrong?** Include the contract + manifest shapes and
  the expected vs actual `Satisfies`.
- **Feature idea?** Check `docs/00_OVERVIEW.md` § "What Almanac is not"
  first — it may be deliberately out of scope.
