# Almanac

> A calendar for agents and their artifacts.
> Subscribe once in any calendar app; every scheduled agent job, the artifact
> it produces, and the artifacts it depends on show up as events — with green
> checks when inputs are ready, red marks when they're not.

**[Website](https://plush-island-rj2a.here.now/)** ·
**[Live demo](https://plush-island-rj2a.here.now/demo/index.html)** ·
**[GitHub](https://github.com/brandongilchrist/almanac)**

Almanac turns scheduled agent work into **standard iCalendar feeds**
(`RFC 5545` / `RFC 7986` / `RFC 9253`). It is **not** a new calendar UI, **not**
a new cron engine, and **not** a new agent runtime. It is the missing
**planning + lineage view** for agent work.

The hard IP is the **data model**: artifact manifests + lineage (the
dependency-checkmark relationship between "this job needs that artifact"),
rendered through a 30-year-old, universally supported open standard.

---

## ⚠️ Read this first: calendar latency is real

ICS feeds are **poll-based**, and clients poll on *their* schedule, not yours:

| Client | Typical poll interval | Worst case seen |
|---|---|---|
| **Google Calendar** | 12–24 hours | up to ~5 days |
| **Apple Calendar** (iCloud) | ~1 hour | a few hours |
| **Outlook.com** | ~3 hours | ~1 day |
| **DAVx⁵ → local CalDAV** | configurable, minutes | minutes |
| **Thunderbird** | configurable, minutes | minutes |

**For near-real-time updates, use the CalDAV path (Phase 2)** or point a
DAVx⁵-style client at a local CalDAV server. The ICS feed is the universal
baseline; it is not the low-latency option.

This is a property of *calendar clients*, not Almanac — we document it loudly
rather than pretend otherwise.

---

## The wedge: dependency checkmarks

Each scheduled agent job declares its **inputs** (artifacts it consumes) and
its **outputs** (artifacts it produces). When a job finishes, Almanac records
a **manifest** for what it produced — content hash, schema version, commit
SHA, timestamp. When a downstream job is about to run, it checks: *does a
materialized manifest exist for each of my inputs?*

- ✅ **ready** — fresh manifest, version matches.
- ❌ **missing** — no manifest within the freshness window.
- ⚠️ **version mismatch** — manifest exists but its version is too old.

This is **data lineage** (the thing Palantir Foundry and Dagster do for data
pipelines) applied to **agent artifacts**. It's encoded as `RELATED-TO;
RELTYPE=DEPENDS-ON` (RFC 9253) so the relationship survives every calendar
sync, with the state mirrored into `STATUS` + emoji `SUMMARY` so every client
renders it.

### Emoji legend

| Emoji | State | Meaning |
|---|---|---|
| 🟡 | `pending` | Scheduled, hasn't run yet this cycle. |
| ⏳ | `running` | Currently executing. |
| ✅ | `succeeded` | Ran, produced a manifest, verified. |
| ❌ | `failed` | Ran, exited non-zero / agent error. |
| ⏸ | `skipped` | Inputs missing or version-mismatched; refused to start. |

So `SUMMARY:✅ 📰 Daily research brief` is universally readable — including
on an Apple Watch.

---

## Quick start

```bash
# Build everything (Rust 1.75+).
cargo build --release

# Run the standalone server with demo data.
./target/release/almanac-server
# → http://localhost:8787/calendar/demo.ics

# Or render the demo feed to stdout without a server.
./target/release/almanac demo > demo.ics
```

Then paste the subscribe URL into any calendar app:

```
http://localhost:8787/calendar/demo.ics
```

You'll see four demo schedules: a daily research brief (✅ succeeded), a
nightly index rebuild (❌ failed), a weekly strategy (🟡 pending, with a ✅
lineage marker showing its input is ready), and a webhook-triggered PR review.

### Subscribe URL formats

| Feed | URL | Contents |
|---|---|---|
| Default | `/calendar/<community>.ics` | Recurring schedules + today's run overlay + lineage. |
| Schedule only | `/calendar/<community>/schedule.ics` | Recurring plans, no run state. |
| Runs only | `/calendar/<community>/runs.ics` | One event per concrete run. |

### Suggested category → color mapping (Google Calendar)

Almanac emits `CATEGORIES:almanac,<group>`. In Google Calendar's settings,
map the group names to colors so different schedule kinds are visually
distinct:

| Group | Suggested color |
|---|---|
| `research` | Blue |
| `strategy` | Green |
| `infra` | Red |
| `code` | Purple |

---

## The CLI

```bash
# Print the subscribe URL for a community.
almanac subscribe --community research
# → http://localhost:8787/calendar/research.ics

# Print the runs-only feed URL.
almanac subscribe --community research --runs

# Check a schedule's input lineage (✅/❌/⚠️ per input).
almanac check weekly-strategy --community demo
# → ✅ ready (v3)  research-brief  ← produced by `daily-brief`

# Emit a KIND_ALMANAC_CONTRACT event body (JSON to stdout).
almanac declare --schedule weekly-strategy --role consume \
  --schema research-brief --min-version 2 --freshness 604800

# Validate an .ics file parses.
almanac validate path/to/feed.ics

# Run the server.
almanac serve
```

---

## The data model

Five Nostr event kinds (parameterized-replaceable by `d`-tag convention):

| Kind | Name | `d` tag | Purpose |
|---|---|---|---|
| `48050` | `KIND_ALMANAC_SCHEDULE` | schedule id | A cron definition with calendar-render hints. |
| `48051` | `KIND_ALMANAC_RUN` | run id | One concrete execution of a schedule. |
| `48052` | `KIND_ALMANAC_MANIFEST` | `<run_id>:<schema_id>` | An artifact's materialization record. **The lineage primitive.** |
| `48053` | `KIND_ALMANAC_CONTRACT` | contract id | A producer/consumer dependency declaration. |
| `48054` | `KIND_ALMANAC_CALENDAR` | calendar id | Calendar grouping/metadata. |

### Decided data-model rules

1. **One manifest per `(run, schema_id)`.** A run producing three artifacts
   emits three manifests. The `d` tag is `<run_id>:<schema_id>`, giving
   correct last-write-wins on replay.
2. **Integer `schema_version` (≥ 1), hard-fail on mismatch.** Consumers
   declare `min_version` (≥ 1). To opt out of version checking, set
   `any_version: true` — do **not** overload `min_version: 0` as a sentinel.
3. **Freshness window is per-contract (default 24h), relative to execution
   time.** A daily producer feeding a weekly consumer sets
   `freshness_window: 604800`.
4. **Webhook-triggered runs render as one-off VEVENTs** on `/runs.ics`,
   tagged `CATEGORIES:webhook`.
5. **Community scope delegates to the channel-read ACL.** Unauthorized
   subscribers see fewer events, not an error.

See [`docs/10_PLAN.md`](docs/10_PLAN.md) for the full spec.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  ANY CALENDAR CLIENT (Google / Apple / Outlook / DAVx⁵ / …)     │
│  Subscribes to one URL. Just renders events. No install.        │
└─────────────────────────────────────────────────────────────────┘
                            ↑ subscribes (one-way ICS feed)
┌─────────────────────────────────────────────────────────────────┐
│  ALMANAC  (Rust)                                                │
│  - Reads cron defs + artifact manifests from events             │
│  - Renders to RFC 5545/7986/9253 iCalendar                      │
│  - Encodes lineage state into STATUS / CATEGORIES / SUMMARY     │
│  - Serves /calendar/<community>.ics (+ /runs.ics, /schedule.ics)│
└─────────────────────────────────────────────────────────────────┘
                            ↑ reads events
┌─────────────────────────────────────────────────────────────────┐
│  EVENT SOURCE (Buzz relay, or any Nostr event producer)         │
└─────────────────────────────────────────────────────────────────┘
```

### Crates

| Crate | What it is |
|---|---|
| [`almanac-bridge`](crates/almanac-bridge) | The core: data model, ingestion, ICS rendering, lineage engine. Pure library, fully tested. |
| [`almanac-server`](crates/almanac-server) | Standalone axum HTTP server + in-memory state store + demo seeder. |
| [`almanac-cli`](crates/almanac-cli) | The `almanac` binary: `subscribe`, `check`, `declare`, `serve`, `demo`, `validate`. |

---

## What Almanac is **not**

To prevent scope creep, the things Almanac deliberately does **not** do:

- **Not a calendar UI.** No web app, no React, no FullCalendar. The view is
  whatever calendar the user already uses.
- **Not a cron engine.** It observes schedules; it does not schedule.
- **Not an agent runtime.** It observes runs; it does not execute.
- **Not real-time by default.** Phase 1 ICS is poll-based. Real-time is a
  Phase 2 CalDAV concern.
- **Not the source of truth.** The event source is. Almanac is a derived
  view; if it crashes, agents keep running.

---

## Development

```bash
cargo fmt --all --check       # format check
cargo clippy --workspace --all-targets -- -D warnings   # lint
cargo test --workspace        # 86 tests, all green
```

The full SDLC (format → clippy → test → build) runs locally via
[`scripts/ci.sh`](scripts/ci.sh) and is mirrored in the local Gitea Actions
pipeline (see [`sdlc/`](sdlc/)). No GitHub Actions minutes consumed.

### Project layout

```
almanac/
├── crates/
│   ├── almanac-bridge/     # core library (data model, ingest, ical, lineage)
│   ├── almanac-server/     # HTTP server + state store + demo seeder
│   └── almanac-cli/        # the `almanac` binary
├── docs/                   # the four planning docs (00–30) + this README's source
├── website/                # static marketing site
├── demo/                   # interactive in-browser demo
├── sdlc/                   # local Gitea + CI pipeline (Docker)
├── scripts/                # ci.sh, dev helpers
└── tests/                  # workspace-level integration tests
```

---

## Roadmap

- **Phase 1** *(this release)* — Read-only ICS bridge. Proves the data model
  and rendering. ✅
- **Phase 2** — CalDAV server (Radicale or roll-your-own) + WebDAV-Push for
  sub-minute updates + `buzz-workflow` pre-fire gating (skip runs with
  unmaterialized inputs).
- **Phase 3** — CalDAV write-back (drag-to-reschedule → schedule update) +
  a server-rendered read-only web view of the lineage DAG.

---

## License

MIT. See [`LICENSE`](LICENSE).

## Acknowledgements

Almanac is designed to plug into the [Buzz](https://github.com/...) platform
but ships standalone so the rendering + lineage core is independently
testable and demonstrable. The data model and ICS contract are documented in
[`docs/`](docs/).
