# Almanac — Goal Ledger

Working file for tracking goal-by-goal progress. Re-orient at the start of
every goal session by reading this file first, then the relevant section of
`10_PLAN.md`, then `git log --oneline -5`.

See `20_META_PLAN.md` for the per-goal workflow and the goal-decomposition
strategy.

---

## Done

_(none yet)_

---

## In Progress

_(none yet)_

---

## Next — Phase 1 (in order)

Each line is one goal. See `20_META_PLAN.md` § "The goal backlog" for full
descriptions and exit criteria.

### Track A — Data model (do first)
- [ ] **G1.** Allocate kind range 48050–48054 in `buzz-core/src/kind.rs` + `is_almanac_kind()` helper + unit tests.
- [ ] **G2.** _(removed — data-model decisions are made in `10_PLAN.md` § "Decided data-model decisions"; nothing to resolve.)_
- [ ] **G3.** Define Rust structs in `crates/almanac-bridge/src/model.rs` + serde round-trip tests.

### Track B — Ingestion (events → structs)
- [ ] **G4.** Schedule ingestion: translate `KIND_WORKFLOW_DEF` (30620) → `Schedule`.
- [ ] **G5.** Run ingestion: translate fire-claim + agent output → `Run` with status.
- [ ] **G6.** Manifest emission (best-effort): parse agent output → `KIND_ALMANAC_MANIFEST`.

### Track C — Rendering (structs → ICS)
- [ ] **G7.** VEVENT rendering — recurring schedule (`RRULE` via `icalendar` + `rrule` crates).
- [ ] **G8.** VEVENT rendering — status overlay (`STATUS` + emoji `SUMMARY` prefix).
- [ ] **G9.** Lineage rendering — `RELATED-TO;RELTYPE=DEPENDS-ON` + `DESCRIPTION` check state.

### Track D — HTTP surface
- [ ] **G10.** `GET /calendar/<community>.ics` route, community-scoped, NIP-42 auth.
- [ ] **G11.** Split feeds: `/runs.ics` alongside `/schedule.ics`.

### Track E — CLI + docs
- [ ] **G12.** `buzz almanac` subcommand: `subscribe`, `check`, `declare`.
- [ ] **G13.** README + honest latency disclaimer (Google 12–24h, Apple ~1h, CalDAV upgrade path).
- [ ] **G14.** End-to-end smoke test in `tests/feed_smoke.rs`.

---

## Discovered (park; promote to Next when ready)

_(none yet)_

---

## Phase 2 draft goals (park; do after Phase 1 done)

- CalDAV server alongside relay (Radicale or roll-your-own with `calcard`).
- WebDAV-Push (RFC 6638 + DAVx⁵ push spec) for sub-minute updates.
- `buzz-workflow` pre-fire gating hook: skip runs with unmaterialized inputs.
- Split schedule/runs feeds with full lineage in `/runs.ics`.

## Phase 3 draft goals (park; do after Phase 2 done)

- CalDAV write-back: drag-to-reschedule → `KIND_ALMANAC_SCHEDULE` update.
- Typed object model surfaced as queryable entities (the "ontology" view).
- Server-rendered read-only web view at `/almanac/` (DAG of schedules → contracts → manifests).
