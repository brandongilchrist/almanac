# Almanac — Goal Ledger

Working file for tracking goal-by-goal progress.

---

## Done

**v0.1.0 — complete working product with full startup surface (2026-07-25)**

The original PROMPT.md scoped Almanac as a patch *inside* the Buzz repo with
no push and no UI. The actual directive broadened that: build a complete,
working, fully-functional product with website, docs, demo, git repo, tests,
and a full local SDLC. Delivered as a standalone Rust workspace that
faithfully implements the spec's data model (kinds 48050–48054), ICS
rendering contract, and lineage engine — without requiring the Buzz codebase.

### Product (Rust, 86 tests green)
- [x] **G1.** Kinds 48050–48054 + `is_almanac_kind()` + compile-time asserts.
- [x] **G3.** `Schedule`/`Run`/`Manifest`/`Contract`/`Calendar` + serde round-trips.
- [x] **G4.** `KIND_WORKFLOW_DEF` → `Schedule` (incl. cron→RRULE: daily/weekly/monthly/multi-day).
- [x] **G5.** Fire-claim → `Run` state machine (Pending→Running→Succeeded/Failed/Skipped) + illegal-transition errors.
- [x] **G6.** Agent output → `Manifest` (GitHub commit/blob URLs, code-fenced paths, `agent-output` fallback, SHA-256 hashes).
- [x] **G7.** VEVENT + RRULE rendering.
- [x] **G8.** Status overlay (emoji SUMMARY + STATUS, all 5 variants).
- [x] **G9.** `RELATED-TO;RELTYPE=DEPENDS-ON` + dependency DESCRIPTION block (✅/❌/⚠️).
- [x] **G10.** `GET /calendar/<community>.ics` + `/runs.ics` + `/schedule.ics` (axum).
- [x] **G11.** Split feeds (schedule-only vs runs-only), both validator-green.
- [x] **G12.** `almanac` CLI: `subscribe`, `check`, `declare`, `serve`, `demo`, `validate`.
- [x] **G13.** README with honest latency disclaimer (Google 12–24h, Apple ~1h, CalDAV path).
- [x] **G14.** End-to-end `feed_smoke.rs` (7 tests: lineage edge, status overlay, ingestion→feed).

### Startup surface
- [x] **Website** — static marketing site, live at https://plush-island-rj2a.here.now/
- [x] **Demo** — interactive in-browser demo mirroring the Rust rendering 1:1 (toggle run states, watch ICS update live).
- [x] **Docs** — README, CHANGELOG, CONTRIBUTING, 4 planning docs (00–30), rustdoc.
- [x] **Git/GitHub** — github.com/brandongilchrist/almanac, CI green.
- [x] **SDLC** — `scripts/ci.sh` (local gate, proven green) + Gitea + Gitea Actions in Docker (no GH minutes) + GitHub Actions mirror.

---

## In Progress

_(none)_

---

## Next — Phase 2 (park; do after Phase 1 settles)

- CalDAV server alongside the relay (Radicale or roll-your-own) for real-time push.
- WebDAV-Push (RFC 6638) for sub-minute updates to DAVx⁵ / Apple Calendar.
- `buzz-workflow` pre-fire gating hook: skip runs with unmaterialized inputs.
- Split schedule/runs feeds with full lineage in `/runs.ics`.

## Phase 3 draft goals (park)

- CalDAV write-back: drag-to-reschedule → `KIND_ALMANAC_SCHEDULE` update.
- Typed object model surfaced as queryable entities (the "ontology" view).
- Server-rendered read-only web view at `/almanac/` (DAG of schedules → contracts → manifests).
