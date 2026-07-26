# Changelog

All notable changes to Almanac follow [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-07-25

The first shippable release: a read-only iCalendar bridge that proves the
data model and the lineage rendering path.

### Added
- **Data model** — `Schedule`, `Run`, `Manifest`, `Contract`, `Calendar`
  structs with serde round-trip tests, plus five Nostr event kinds
  (`48050`–`48054`) and an `is_almanac_kind()` helper.
- **Ingestion** — `KIND_WORKFLOW_DEF` (30620) → `Schedule` (incl. cron→RRULE),
  fire-claim → `Run` state machine (`Pending → Running → Succeeded | Failed |
  Skipped`), and best-effort agent-output → `Manifest` emission (GitHub
  commit URLs, blob URLs, code-fenced file paths, with an `agent-output`
  fallback).
- **Lineage engine** — `check_inputs` against a `ManifestStore` trait with
  freshness-window + version rules (`Ready` / `Missing` / `VersionMismatch`),
  plus `derive_edges` for the producer↔consumer graph.
- **ICS rendering** — VEVENT + RRULE, status overlay (`STATUS` + emoji
  `SUMMARY`), `RELATED-TO;RELTYPE=DEPENDS-ON` (RFC 9253), `CATEGORIES`, and
  the feed container with `X-WR-CALNAME`. All output parses with the
  `icalendar` crate.
- **HTTP server** — standalone axum server serving `/calendar/<community>.ics`,
  `/calendar/<community>/schedule.ics`, `/calendar/<community>/runs.ics`,
  plus ingestion endpoints and a JSON state/lineage API. Demo data seeded on
  startup.
- **CLI** — `almanac subscribe`, `check`, `declare`, `serve`, `demo`,
  `validate`.
- **Tests** — 86 tests across the workspace, including an end-to-end
  `feed_smoke.rs` that asserts on the full lineage-checkmark flow.
- **Docs** — README with the latency disclaimer, four planning docs
  (`00`–`30`), CONTRIBUTING guide, this changelog.
- **Website + demo** — static marketing site and an interactive in-browser
  demo that mirrors the Rust rendering 1:1.
- **SDLC** — local Gitea + Gitea Actions pipeline (Docker) mirroring
  `scripts/ci.sh`, so the full quality gate runs with zero GitHub Actions
  minutes. A GitHub Actions mirror is included for hosted use.

### Known limitations
- ICS feeds are poll-based; Google Calendar polls every 12–24h. Documented
  prominently in the README. Real-time is a Phase 2 CalDAV concern.
- No major calendar client visually renders `RELATED-TO;RELTYPE=DEPENDS-ON`.
  The lineage state is therefore also mirrored into `STATUS` + emoji
  `SUMMARY` so every client shows it.

### Out of scope for this release (by design)
- Real-time push (CalDAV / WebDAV-Push) — Phase 2.
- Write-back (calendar as a control surface) — Phase 3.
- Auto-discovery of contracts from agent source code.
- A JavaScript web app — the user's calendar is the UI.

[0.1.0]: https://github.com/brandon-gilchrist/almanac/releases/tag/v0.1.0
