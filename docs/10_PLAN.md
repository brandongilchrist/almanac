# Almanac — Implementation Plan

This is the *how*. Concrete phases, kind numbers, file layout, data model,
and integration points. Read `00_OVERVIEW.md` first if you haven't.

---

## Guiding principles

1. **Read-mostly first.** Phase 1 is a derived view; it never mutates Buzz
   state except to emit lineage events. The calendar is the user's; Almanac
   just writes events into a feed.
2. **No new infrastructure.** No new database, no new process, no new port.
   Almanac runs inside the Buzz relay process and reuses its HTTP surface,
   event store, and auth pipeline.
3. **Buzz-native.** All state is Nostr events in dedicated kinds. Nothing
   lives in Almanac's own store. If Almanac is uninstalled, the events
   remain — they're just Buzz events with custom kinds.
4. **Standard protocols only.** iCalendar (RFC 5545/7986/9253) on the way
   out; Nostr (NIP-01/29/33/42) on the way in. No new wire formats.
5. **Honest about real-time.** Phase 1 ICS is poll-based and slow on Google
   Calendar. Don't pretend otherwise; document the latency in the README.

---

## Event kinds

Allocated in `buzz-core/src/kind.rs`. Range **48050–48099** is currently
unused (sits between the `48100–48106` cluster and the `48999` sentinel).
All are **parameterized replaceable** (NIP-33) unless noted — LWW semantics
match how schedules and manifests naturally update.

| Kind | Name | Replaceable? | Purpose |
|---|---|---|---|
| `48050` | `KIND_ALMANAC_SCHEDULE` | yes (NIP-33, `d` = schedule id) | A cron definition as seen by Almanac. Mirror of `buzz-workflow` def with calendar-render hints (color, summary template, calendar subgroup). |
| `48051` | `KIND_ALMANAC_RUN` | yes (NIP-33, `d` = run id) | One concrete execution of a schedule. Carries `scheduled_for`, `started_at`, `finished_at`, `status`, output manifest pointer. |
| `48052` | `KIND_ALMANAC_MANIFEST` | yes (NIP-33, `d` = manifest id) | An artifact's materialization record. Carries `producer_run`, `content_hash`, `schema_id`, `schema_version`, `commit_sha`, `uri`, `bytes`, `materialized_at`. **The lineage primitive.** |
| `48053` | `KIND_ALMANAC_CONTRACT` | yes (NIP-33, `d` = contract id) | A producer/consumer contract: declares what a schedule *expects to produce* or *expects to consume* (schema id + version). The static dependency declaration. |
| `48054` | `KIND_ALMANAC_CALENDAR` | yes (NIP-33, `d` = calendar id) | Calendar grouping/metadata: name, color, description, which schedules belong. Lets a community publish multiple calendars (e.g. "schedule" vs "runs" vs "deps only"). |

Run state lives in `KIND_ALMANAC_RUN.status` tag — one of `pending`,
`running`, `succeeded`, `failed`, `skipped`, `degraded`. Maps directly to
iCalendar `STATUS` (`TENTATIVE` / `CONFIRMED` / `CANCELLED`) and
`CATEGORIES` (for color rendering).

### Lineage is just events

A schedule that consumes an artifact declares it via a `KIND_ALMANAC_CONTRACT`
event with tags `contract_role:consume`, `schema_id:<id>`,
`schema_version:<min>`. A schedule that produces one declares
`contract_role:produce`. The "is my input ready?" check is a Nostr query:

```
filter { kinds: [48052], schema_id: <expected>, limit: 1, since: <cutoff> }
```

If a manifest exists with matching `schema_id` and `schema_version >= min`
within the cutoff window, the input is ✅. Otherwise ✗.

This is the entire lineage engine. No graph database, no scheduler. Just Nostr
filters on events emitted by job runners.

---

## Crate layout

Almanac ships as a single crate, `almanac-bridge`, with optional features. It
plugs into `buzz-relay` the same way `buzz-search`, `buzz-workflow`, and
`buzz-media` do — as an internal module wired into the relay's HTTP router
and event pipeline.

```
crates/almanac-bridge/
  Cargo.toml
  README.md
  src/
    lib.rs                  # public API: serve_calendar(), observe_event()
    config.rs               # env vars, feature flags
    kinds.rs                # KIND_ALMANAC_* constants (mirror of buzz-core)
    model.rs                # Schedule, Run, Manifest, Contract, Calendar structs
    ical/
      mod.rs
      event.rs              # VEVENT rendering (RRULE, STATUS, CATEGORIES)
      related.rs            # RELATED-TO;RELTYPE=DEPENDS-ON emission
      feed.rs               # Calendar == ordered list of VEVENTs + X-WR-CALNAME
    lineage/
      mod.rs
      check.rs              # "is input X materialized?" query
      graph.rs              # derive consumer→producer edges from contracts
    http.rs                 # axum routes: GET /calendar/<id>.ics, /runs.ics, /deps.ics
    observe.rs              # ingest buzz-workflow + agent events → emit Run/Manifest
  tests/
    ical_render.rs          # assert feeds parse with the icalendar crate
    lineage_check.rs        # manifest-present / manifest-missing / version-mismatch
    feed_smoke.rs           # end-to-end: emit events, GET /calendar.ics, parse
```

Dependencies (kept lean):

- `icalendar` (crates.io) — VEVENT construction.
- `rrule` (crates.io) — RRULE expansion for one-off run events.
- Existing workspace crates: `buzz-core`, `buzz-db`, `buzz-workflow`
  (read-only access to def + fire events).
- `axum` (already in relay) — HTTP routes.

**No new external services.** No Postgres extensions, no Redis, no CalDAV
server in Phase 1.

---

## Integration into buzz-relay

Three concrete hooks, all matching how existing crates integrate:

1. **Event observation** — subscribe to `KIND_WORKFLOW_DEF` (`30620`),
   `KIND_WORKFLOW_FIRE` (the durable fire-claim event buzz-workflow emits),
   and agent output events. Translate each into the corresponding
   `KIND_ALMANAC_*` event. This is the only place Almanac *writes*.

2. **HTTP route** — register `GET /calendar/<community>.ics` (and
   `/calendar/<community>/runs.ics`, `/deps.ics`) on the relay's existing
   router. Same NIP-42 / community-membership auth as everything else. The
   ICS endpoint is community-scoped, just like `POST /events`.

3. **Workflow hook (optional, later)** — a pre-fire check in
   `buzz-workflow`'s trigger path: "does this run have all its declared
   input manifests?" If no, mark `status:skipped` with reason
   `missing_input:<schema_id>` instead of running. **Phase 2 only** —
   Phase 1 just observes and renders; it does not gate.

---

## Phases

### Phase 1 — Read-only ICS bridge (the weekend that proves it)

**Goal:** paste one URL into Google Calendar, see your scheduled agent work
as recurring events with status encoded in `STATUS` + emoji-prefixed
`SUMMARY`. Proves the data model and the rendering path.

**Scope:**

- `KIND_ALMANAC_SCHEDULE` ingestion from `buzz-workflow` defs.
- `KIND_ALMANAC_RUN` ingestion from observed fires + agent output.
- `KIND_ALMANAC_MANIFEST` emission (best-effort — parse agent output for
  artifact references; if none detectable, emit a manifest pointing at the
  agent's output thread).
- `KIND_ALMANAC_CONTRACT` (declare-only — manual config in v1; users write
  these by hand or via a CLI helper).
- ICS feed at `/calendar/<community>.ics` rendering Schedules as recurring
  `VEVENT`s with `RRULE`, plus today's Run state overlaid via `STATUS` and
  emoji `SUMMARY` prefix.
- A tiny `almanac` CLI subcommand in `buzz-cli`: `almanac subscribe` prints
  the URL; `almanac check <schedule>` prints lineage state.

**Deliberately out of Phase 1:**

- Real-time push (CalDAV).
- Write-back (calendar as control surface).
- Auto-discovery of contracts from agent source code.
- Multi-calendar grouping (`KIND_ALMANAC_CALENDAR`).
- Per-instance "run" events (Phase 1 uses the recurring event + status
  overlay model).

**Exit criteria:**

1. Running `curl http://localhost:3000/calendar/<my-community>.ics` returns
   a feed that passes the [icalendar.org validator](https://icalendar.org/validator.html).
2. Subscribing in Apple Calendar shows your schedules as events.
3. Triggering a schedule manually causes its `STATUS` to flip from
   `TENTATIVE` to `CONFIRMED` (within Apple Calendar's ~1h poll).
4. A schedule with a declared `consume` contract shows ✅ when the producer
   manifest exists, ✗ when it doesn't.

### Phase 2 — CalDAV server + dependency gating

**Goal:** real-time updates for power users, plus the calendar actually
*blocks* jobs whose inputs aren't ready.

**Scope:**

- Run a CalDAV server alongside the relay. Pick one:
  - [Radicale](https://github.com/Kozea/Radicale) (Python, simplest) behind
    the same auth as the relay, or
  - Roll a minimal CalDAV subset in Rust using [`calcard`](https://www.reddit.com/r/rust/comments/1na5vrc/)
    + a thin axum layer, reusing Buzz's auth.
- Implement [WebDAV-Push](https://manual.davx5.com/webdav_push.html) so
  DAVx⁵ / Apple Calendar get sub-minute updates.
- Wire the `buzz-workflow` pre-fire hook: a schedule with unmaterialized
  inputs is marked `skipped:missing_input` rather than run. Surface this
  state in the ICS feed (red ✗, distinct from "ran and failed").
- Split the feed: `/schedule.ics` (recurring plan) and `/runs.ics`
  (one event per concrete run with full lineage).

**Exit criteria:**

1. DAVx⁵ connected, drag-to-reschedule in Apple Calendar round-trips within
   seconds (CalDAV write-back working).
2. A schedule whose producer failed does not fire; its event shows
   `STATUS:CANCELLED` with reason in `DESCRIPTION`.
3. Real-time: status flip visible in Apple Calendar in under 60s.

### Phase 3 — Ontology surface + write-back

**Goal:** the calendar as a *control surface*, not just a view.

**Scope:**

- CalDAV write handler: drag an event → updates `KIND_ALMANAC_SCHEDULE`.
  Mark an event `STATUS:CANCELLED` → cancels the next run.
- Typed object model: `Contract`, `Manifest`, `Schedule`, `Run` become
  first-class queryable objects with relations (the "ontology" surface,
  conceptually analogous to Palantir Foundry's typed objects but rendered
  through Nostr events + a query CLI).
- A read-only web view (no JS app — server-rendered) at `/almanac/` showing
  the lineage DAG, for when the calendar view isn't enough. Not a
  replacement for the ICS feed; a complement.

**Exit criteria:**

1. Rescheduling a cron from Apple Calendar updates the Buzz schedule.
2. `/almanac/` shows a DAG of schedules → contracts → manifests with
   materialization state.
3. The data model has stayed Buzz-native throughout (no foreign DB).

---

## The ICS rendering contract

The single most important implementation detail. Pin this down before writing
any rendering code; every field choice is a UX decision because clients
render different fields.

| Hermes concept | iCalendar field | Why this field |
|---|---|---|
| Schedule (the cron) | `VEVENT` + `RRULE` | Universally rendered. |
| Schedule name | `SUMMARY` | Prefixed with state emoji (see below). |
| Schedule description | `DESCRIPTION` | Markdown body; most clients render OK. |
| Schedule color | `CATEGORIES` | Google/Apple map categories → colors. Use `almanac:<group>` for stable colors. |
| Expected output artifact | `ATTACH` + `STRUCTURED-DATA` (RFC 7986) | `STRUCTURED-DATA` carries schema id + version. |
| Dependencies | `RELATED-TO;RELTYPE=DEPENDS-ON:<uid>` | RFC 9253. Stored universally; rendered by few. |
| Run state | `STATUS` | `TENTATIVE` (pending/ready), `CONFIRMED` (succeeded), `CANCELLED` (failed/skipped). |
| Run reason | `DESCRIPTION` append | "Inputs ready at …" / "Failed: exit 1" / "Skipped: missing …". |
| Output artifact URL | `URL` + `ATTACH` | Link to GitHub commit / Buzz thread. |

**Emoji prefix scheme** (lowest-tech, highest-compatibility signal — shows up
everywhere including Apple Watch):

- 🟡 `pending` — scheduled, hasn't run yet this cycle.
- 🟢 `ready` — inputs verified, will run.
- ⏳ `running` — currently executing.
- ✅ `succeeded` — ran, produced manifest, verified.
- ❌ `failed` — ran, exited non-zero / agent error.
- ⏸ `skipped` — inputs missing, refused to start.

So `SUMMARY:✅ 📰 Daily research brief` is universally readable.

**Honest limits to bake into the README from day one:**

- Google Calendar polls every 12–24h (sometimes up to 5 days). Real-time is
  Phase 2 / CalDAV only.
- No major client visually renders `RELATED-TO;RELTYPE=DEPENDS-ON`. The
  relationship is stored but not drawn — that's why we encode state into
  `STATUS` + `SUMMARY` emoji too.
- ICS feeds are read-only. Editing in the calendar goes nowhere until
  Phase 3.

---

## Testing strategy

Three layers, matching Buzz's existing test conventions:

1. **Unit tests** in each module — ICS rendering, lineage queries, event
   translation. Pure functions; no I/O. Run with `just test-unit`.
2. **Integration tests** in `crates/almanac-bridge/tests/` — stand up a
   test relay, emit events, hit the HTTP endpoints, parse the returned ICS
   with the `icalendar` crate to assert it's well-formed and contains the
   expected VEVENTs. Run with `just test` (requires Postgres + Redis, like
   the rest of the integration suite).
3. **CalDAV interop tests** (Phase 2 only) — a small suite that points
   DAVx⁵ / a CalDAV client library at the running server and asserts sync
   round-trips. Lived in `tests/caldav_interop.rs`.

**Mandatory for every feed change:** run the ICS output through
[icalendar.org/validator.html](https://icalendar.org/validator.html). Most
"works in Google, breaks in Apple" bugs are spec-conformance issues this
catches in seconds.

---

## Decided data-model decisions

These are **decisions, not open questions.** Implement exactly as
specified. If a decision turns out to be genuinely unworkable, write to
`BLOCKERS.md` and stop, rather than silently picking a different answer.

1. **Manifest identity — one per (run, schema_id).** A run that produces
   three distinct artifacts emits three manifests. The NIP-33 `d` tag is
   `<run_id>:<schema_id>`. This keeps versioning per-artifact instead of
   forcing one version across a heterogeneous output set, and gives
   correct last-write-wins semantics on replay (re-emitting the same
   artifact for the same run updates it in place rather than duplicating).
   **Contract:** a run emitting two manifests with the same `schema_id`
   is a producer bug; the second emission overwrites the first via
   NIP-33 LWW. Producers that legitimately produce multiple artifacts of
   the same schema must use distinct `schema_id`s.

2. **Contract versioning — integer `schema_version` (≥ 1), hard-fail on
   mismatch.** Producer declares `schema_version` (monotonic integer,
   starting at 1). Consumer declares `min_version` (≥ 1). Materialization
   is valid **iff** `manifest.schema_version >= contract.min_version`.
   A run whose input is satisfied only by an older version is
   `skipped:version_mismatch`. To opt out of version checking entirely,
   the consumer sets a separate `any_version: bool` field (default
   `false`) — do **not** overload `min_version: 0` as a sentinel;
   versions start at 1 and `min_version` is always ≥ 1 unless
   `any_version: true`.

3. **Freshness window — per-contract, default 24h, relative to execution
   time (not scheduled time).** Each contract has a `freshness_window`
   tag in seconds (default `86400`). The lineage check runs at consumer
   execution time `now` (which is `>= scheduled_for`, since the consumer
   is firing). Materialization is valid **iff**
   `(now - freshness_window) <= manifest.materialized_at <= now`.
   Bounding against `now` (not `scheduled_for`) is intentional: a
   producer that finishes at 4:30pm is valid for a consumer scheduled
   for 4:00pm but firing late at 5:00pm — fresher is better, and the
   consumer hasn't run yet. Daily producers feeding weekly consumers:
   set `freshness_window: 604800` on the consuming contract. Weekly
   producers feeding daily consumers: set `freshness_window: 86400`
   (the default) — the daily consumer sees the most recent weekly
   manifest within the last 24h.

4. **Webhook-triggered runs — yes, render as one-off VEVENTs.** They
   appear on `/runs.ics` only (never on `/schedule.ics`), tagged
   `CATEGORIES:webhook`. A webhook run with no `RRULE` is a single
   `VEVENT` at its `started_at`.

5. **Community scope — delegate to the existing channel-read ACL.**
   `/calendar/<community>.ics` omits any schedule whose channel is
   private to a pubkey set the requesting subscriber isn't in. The
   check is the same one the relay uses for `POST /query` against a
   private channel — call into that existing function; do not
   reimplement ACL logic. Auth is NIP-42 on the HTTP request, same as
   every other community-scoped endpoint. Unauthorized subscribers see
   a feed with fewer events, not an error.

---

## Out of scope (for now)

Explicit non-goals to prevent drift:

- A web UI. (Phase 3 read-only view is the only exception, and it's
  server-rendered, not a JS app.)
- Replacing `buzz-workflow`. Almanac reads its events; it does not replace
  the cron engine.
- Running agents. `buzz-acp` + Goose (or any ACP agent) does that.
- A new auth model. Reuse NIP-42 + community membership.
- Mobile app. The user's existing calendar app *is* the mobile app.
- Notifications. Buzz already has notification primitives (`KIND_PUSH_LEASE`,
  `KIND_EVENT_REMINDER`); Almanac does not add new ones.

---

## Risk register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Google Calendar's ICS lag makes the product feel broken | High | High | Document loudly; ship Phase 2 CalDAV path ASAP; recommend Apple Calendar / DAVx⁵ in README. |
| `buzz-workflow` events change shape under us | Medium | High | Pin to specific event versions; add a contract test in CI that asserts the shape we depend on. |
| Major clients ignore `RELATED-TO;RELTYPE=DEPENDS-ON` visually | Certain | Medium | Already mitigated — state is also in `STATUS` + emoji `SUMMARY`. Don't depend on the relationship being drawn. |
| CalDAV server (Phase 2) is a heavy lift | Medium | Medium | Defer to Phase 2; Phase 1 ships without it. If Phase 2 is too big, ship Radicale behind the relay instead of rolling our own. |
| Artifact manifests require parsing agent output (messy) | High | Medium | Phase 1: best-effort, point manifest at agent output thread. Phase 2+: agents emit manifests directly via a small MCP tool / convention. |
| Scope creep into "build a new orchestrator" | Medium | High | This doc's "Out of scope" section is the contract. Re-read it every goal session. |
