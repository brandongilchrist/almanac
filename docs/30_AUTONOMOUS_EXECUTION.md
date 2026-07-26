# Almanac — Autonomous Execution Spec

This document is the **single source of truth for an unattended overnight
build of Almanac Phase 1.** It replaces the 14-goal prompt-and-submit cycle
in `20_META_PLAN.md`. A single agent context, given this file plus the
overview and plan, executes until Phase 1 is done or cleanly blocked.

It is written for a context that will not have a human available to answer
questions. **Every decision is made.** Every ambiguity is resolved. Every
blocker has a defined response.

Read order for the executing agent:
1. `00_OVERVIEW.md` — what Almanac is and is not.
2. `10_PLAN.md` — data model, kinds, ICS contract, phases.
3. `20_META_PLAN.md` — conventions, working-in-Buzz rules.
4. This file — the execution loop.

---

## Operating principles (read first; bind on every decision)

1. **You are not the architect.** The architecture is decided in
   `10_PLAN.md`. Your job is to implement it faithfully. If you believe a
   decision is wrong, write to `BLOCKERS.md` and continue with the
   documented decision; do not silently substitute your own.

2. **The plan is the source of truth, not memory.** When you're unsure of
   a shape, a kind number, or a field name, re-read the plan. Do not guess
   from recall. Kinds live at 48050–48054 (5 kinds, exact names in
   `10_PLAN.md` § "Event kinds").

3. **Verify or stop.** Every change must end in either a green check
   (`cargo test`, ICS validates) or a documented blocker. **Never commit
   red.** Never commit "I'll fix the tests next." If you can't make a
   test pass, write the failure to `BLOCKERS.md`, revert to the last
   green state, and move to the next non-blocked task.

4. **One commit per task.** Small, focused, fully verified. Commit
   message format: `feat(almanac): …`, `fix(almanac): …`,
   `test(almanac): …`, `docs(almanac): …`. Match existing buzz history
   (`git log --oneline -20`).

5. **Stay in scope.** Re-read `00_OVERVIEW.md` § "What Almanac is not"
   before every task. If a task seems to require building a web UI,
   replacing `buzz-workflow`, running agents, or adding a new protocol,
   **stop** — you've drifted. Write to `BLOCKERS.md`.

6. **Don't push or open a PR unless explicitly instructed.** This is in
   the global guidance. It applies here. Local commits on the working
   branch only.

7. **Document drift immediately.** If the plan and reality diverge
   (e.g., `buzz-workflow`'s event shape isn't what `10_PLAN.md` claims),
   fix the plan first, then implement against the fixed plan. Don't
   implement against a stale plan.

---

## Pre-flight (do once, before any code)

Run these in order. Do not skip.

1. **Activate Hermit** from the repo root:
   ```bash
   . ./bin/activate-hermit
   ```

2. **Confirm starting state:**
   ```bash
   cd ~/PROJECTS/BUZZ && git status && git log --oneline -5
   ```
   Working tree should be clean. Note the current branch; you'll branch
   off it.

3. **Create the working branch:**
   ```bash
   git switch -c feat/almanac-phase-1
   ```

4. **Verify the kind range is free** (sanity check the plan):
   ```bash
   grep -nE "4805[0-9]|4806[0-9]|4807[0-9]|4808[0-9]|4809[0-9]" \
     crates/buzz-core/src/kind.rs
   # Expected: no output (range is free).
   ```

5. **Read the relevant existing patterns** so your code matches:
   - How `buzz-search` registers HTTP routes in `buzz-relay`.
   - How `buzz-workflow` declares its event kinds.
   - How an existing integration test in `crates/buzz-test-client/tests/`
     stands up a relay.
   Delegate to a subagent if the search would burn main-context tokens.

6. **Initialize `BLOCKERS.md`:**
   ```bash
   echo "# Almanac — Blockers Log" > ~/PROJECTS/almanac/BLOCKERS.md
   echo "" >> ~/PROJECTS/almanac/BLOCKERS.md
   echo "Format: [task-id] [timestamp] [symptom] [decision-or-next-step]" \
     >> ~/PROJECTS/almanac/BLOCKERS.md
   ```

7. **Initialize the run log:**
   ```bash
   echo "# Almanac — Run Log" > ~/PROJECTS/almanac/RUN_LOG.md
   echo "" >> ~/PROJECTS/almanac/RUN_LOG.md
   ```
   Append one line per task completed: `[task-id] [status] [commit]`.

---

## The task list (execute in order)

Each task is atomic: read spec → implement → verify → commit → log. Do
not start the next task until the current one is committed (or blocked
and reverted).

Tasks are grouped into tracks only for readability. **Execute strictly in
the numbered order** — later tasks depend on earlier ones.

### Track A — Data model

#### T1. Allocate event kinds

**Spec:** Add to `crates/buzz-core/src/kind.rs`, in numeric order, in a
new "Almanac" section:

```rust
// Almanac — calendar + artifact lineage (48050–48099)
pub const KIND_ALMANAC_SCHEDULE: u32 = 48050;
pub const KIND_ALMANAC_RUN: u32 = 48051;
pub const KIND_ALMANAC_MANIFEST: u32 = 48052;
pub const KIND_ALMANAC_CONTRACT: u32 = 48053;
pub const KIND_ALMANAC_CALENDAR: u32 = 48054;

/// Returns true if `kind` is an Almanac-managed kind.
pub const fn is_almanac_kind(kind: u32) -> bool {
    matches!(
        kind,
        KIND_ALMANAC_SCHEDULE
            | KIND_ALMANAC_RUN
            | KIND_ALMANAC_MANIFEST
            | KIND_ALMANAC_CONTRACT
            | KIND_ALMANAC_CALENDAR
    )
}
```

Add `const _: () = assert!(is_parameterized_replaceable(...))` checks
for each, matching the existing pattern at the bottom of the file
(see lines ~783–790 for examples).

**Verify:**
```bash
cargo test -p buzz-core --lib kind
cargo clippy -p buzz-core -- -D warnings
cargo fmt -p buzz-core
```

**Commit:** `feat(core): allocate Almanac event kinds 48050–48054`

---

#### T2. Scaffold the `almanac-bridge` crate

**Spec:** Create `crates/almanac-bridge/` with:

- `Cargo.toml` — workspace member; deps: `buzz-core` (workspace),
  `serde`, `serde_json`, `thiserror`, `tracing` (all workspace).
  Add `icalendar` and `rrule` as direct deps — pin to current versions
  from crates.io (do not guess; check crates.io for the latest stable).
- `src/lib.rs` — module declarations, re-exports.
- `src/kinds.rs` — re-export the `KIND_ALMANAC_*` constants from
  `buzz-core` (single source of truth; do not redefine the numbers).
- `src/model.rs` — empty module with a `// TODO T3` marker.
- `README.md` — one-paragraph description (matches the overview).

Register in the workspace `Cargo.toml` (root) the same way other
`buzz-*` crates are registered.

**Verify:**
```bash
cargo build -p almanac-bridge
cargo clippy -p almanac-bridge -- -D warnings
cargo fmt -p almanac-bridge
```

**Commit:** `feat(almanac): scaffold almanac-bridge crate`

---

#### T3. Define Rust structs

**Spec:** In `crates/almanac-bridge/src/model.rs`, define these types,
following the decisions in `10_PLAN.md` § "Resolved data-model
decisions":

```rust
pub struct Schedule { /* schedule_id, community_id, channel_id,
                         summary, description, rrule, calendar_group,
                         color_category, created_at, updated_at */ }

pub struct Run {
    pub run_id: String,
    pub schedule_id: String,
    pub scheduled_for: i64,      // unix seconds
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub status: RunStatus,
    pub error: Option<String>,
}

pub enum RunStatus { Pending, Running, Succeeded, Failed, Skipped(SkipReason) }

pub enum SkipReason { MissingInput(String), VersionMismatch(String) }

pub struct Manifest {
    pub manifest_id: String,        // == run_id:schema_id
    pub run_id: String,
    pub schema_id: String,
    pub schema_version: u32,
    pub content_hash: String,       // sha256 hex
    pub commit_sha: Option<String>,
    pub uri: String,                // github URL / buzz thread / file path
    pub bytes: Option<u64>,
    pub materialized_at: i64,
}

pub struct Contract {
    pub contract_id: String,
    pub schedule_id: String,
    pub role: ContractRole,         // Produce | Consume
    pub schema_id: String,
    pub min_version: u32,           // for Consume; >= 1 (versions start at 1)
    pub any_version: bool,          // for Consume; true = skip version check
    pub freshness_window: u64,      // seconds; default 86400
}

pub enum ContractRole { Produce, Consume }

pub struct Calendar {
    pub calendar_id: String,
    pub community_id: String,
    pub name: String,
    pub description: String,
    pub color: Option<String>,
    pub schedule_ids: Vec<String>,
}
```

Add serde derives (`Serialize`, `Deserialize`) on all. Add a unit test
per type that round-trips through `serde_json` and back. Add a test
asserting `Manifest::id_for(run, schema)` produces `"{run}:{schema}"`.

**Verify:** `cargo test -p almanac-bridge model`

**Commit:** `feat(almanac): add Schedule/Run/Manifest/Contract/Calendar model`

---

### Track B — Ingestion

#### T4. Schedule ingestion

**Spec:** In `crates/almanac-bridge/src/ingest/`, create:

- `schedule.rs` — a pure function `pub fn schedule_from_workflow_def(event: &Event) -> Result<Schedule, IngestError>`.
  Maps `KIND_WORKFLOW_DEF` (30620) fields to `Schedule`. Read the actual
  shape of 30620 from `crates/buzz-workflow/` source (do not guess).

**If the shape of `KIND_WORKFLOW_DEF` is not what you expected**
(common overnight failure mode): write a contract test
(`tests/workflow_def_shape.rs`) that asserts the tag names you depend
on, then implement against the actual shape. Update `10_PLAN.md` if
the plan's claims were wrong.

**Verify:** unit tests with fixture events covering: standard daily
cron, weekly cron, webhook trigger.

**Commit:** `feat(almanac): ingest KIND_WORKFLOW_DEF → Schedule`

---

#### T5. Run ingestion

**Spec:** In `crates/almanac-bridge/src/ingest/run.rs`:

- `pub fn run_from_fire_claim(...) -> Result<Run, IngestError>` — maps
  the durable fire-claim event from `buzz-workflow` to a `Run` with
  `status: Pending`.
- `pub fn transition(run: &mut Run, event: &Event) -> Result<(), IngestError>`
  — applies an agent output / completion event, advancing
  `Pending → Running → Succeeded | Failed | Skipped`.

State machine:
```
Pending ──start──→ Running ──ok──→ Succeeded
                      │
                      └──err──→ Failed
Pending ──skip──→ Skipped(reason)
```

**Verify:** unit tests covering every legal transition and one illegal
transition (must error).

**Commit:** `feat(almanac): ingest workflow fire-claims → Run state machine`

---

#### T6. Manifest emission

**Spec:** In `crates/almanac-bridge/src/ingest/manifest.rs`:

- `pub fn manifest_from_agent_output(run: &Run, event: &Event) -> Result<Vec<Manifest>, IngestError>`
  — best-effort parse of an agent's output event for artifact references:
  - GitHub commit URLs (`https://github.com/.../commit/<sha>` or
    `/blob/<sha>/...`).
  - File paths in code fences.
  - If nothing parseable, emit a single manifest with `schema_id:
    "agent-output"`, `uri` = the Buzz thread URL for the run.

Each `Manifest` must have `manifest_id = format!("{run_id}:{schema_id}")`,
`materialized_at = now`, `content_hash = sha256(event.content)`.

**Verify:** unit tests with three fixtures: GitHub commit URL, file
path in code fence, no parseable artifacts (fallback path).

**Commit:** `feat(almanac): emit Manifests from agent output (best-effort)`

---

### Track C — Rendering

#### T7. VEVENT rendering — recurring schedule

**Spec:** In `crates/almanac-bridge/src/ical/event.rs`:

- `pub fn render_schedule_vevent(schedule: &Schedule) -> Result<Component, IcalError>`
  — produces an `icalendar::Calendar` (or `Component`) with:
  - `UID` = `<schedule_id>@almanac`
  - `SUMMARY` = schedule summary
  - `DESCRIPTION` = schedule description
  - `DTSTART` + `RRULE` from the schedule's cron (use `rrule` crate to
    expand; the VEVENT carries `RRULE` for the recurrence).
  - `CATEGORIES` = `["almanac", schedule.calendar_group]`
  - `STATUS` = `TENTATIVE` (default for schedules; per-run state
    overrides via T8).

Use the `icalendar` crate's builder API. Do not hand-roll ICS strings.

**Verify:** unit test asserts the rendered component parses via
`icalendar::Calendar::from_str` and contains the expected fields. Then
render to a string and paste into
[icalendar.org/validator.html](https://icalendar.org/validator.html) —
must be valid.

**Commit:** `feat(almanac): render Schedule → VEVENT with RRULE`

---

#### T8. VEVENT rendering — status overlay

**Spec:** In `crates/almanac-bridge/src/ical/event.rs`:

- `pub fn overlay_run_status(vevent: &mut Component, run: Option<&Run>)`
  — if `run` is `Some`, mutate the VEVENT:
  - `STATUS` per the mapping table in `10_PLAN.md`:
    `Pending → TENTATIVE`, `Running → TENTATIVE`, `Succeeded → CONFIRMED`,
    `Failed → CANCELLED`, `Skipped → CANCELLED`.
  - `SUMMARY` prefix with emoji per `10_PLAN.md`:
    `Pending → 🟡`, `Running → ⏳`, `Succeeded → ✅`, `Failed → ❌`,
    `Skipped → ⏸`.
  - `DESCRIPTION` append: run state, `materialized_at`, error reason
    if any.

**Verify:** unit tests — one per `RunStatus` variant — asserting the
output `STATUS`, the emoji prefix in `SUMMARY`, and the appended
description.

**Commit:** `feat(almanac): overlay Run status into VEVENT`

---

#### T9. Lineage rendering

**Spec:** In `crates/almanac-bridge/src/ical/related.rs`:

- `pub fn render_dependency(vevent: &mut Component, deps: &[Dependency])`
  where `Dependency { schedule_id: String, satisfied: Satisfies }` and
  `enum Satisfies { Ready, Missing, VersionMismatch }`.
  - Adds `RELATED-TO;RELTYPE=DEPENDS-ON:<schedule_id>@almanac` for each
    dep (RFC 9253).
  - Appends a "Dependencies:" block to `DESCRIPTION`:
    `✅ dep1@almanac (materialized <time>)` / `❌ dep2@almanac
    (no manifest in freshness window)` / `⚠️ dep3@almanac (v5 found,
    need v7+)`.

The `deps` are computed by the lineage checker (T13), not by the
renderer — the renderer is pure.

**Verify:** unit tests per `Satisfies` variant. Validator-green output.

**Commit:** `feat(almanac): render dependencies via RELATED-TO;RELTYPE=DEPENDS-ON`

---

### Track D — Lineage engine

#### T10. Lineage checker

**Spec:** In `crates/almanac-bridge/src/lineage/check.rs`:

- `pub async fn check_inputs(store: &impl ManifestStore, run: &Run, contracts: &[Contract]) -> Result<Vec<Dependency>, LineageError>`
  — for each `Contract { role: Consume, schema_id, min_version,
  any_version, freshness_window }`:
  - Let `now = run.started_at.unwrap_or(run.scheduled_for)` (the
    consumer's execution time; falls back to scheduled time if the
    run hasn't started).
  - Query the store for the most recent manifest matching `schema_id`
    with `(now - freshness_window) <= materialized_at <= now`.
  - If none: `Satisfies::Missing`.
  - If `any_version` is false and `manifest.schema_version <
    min_version`: `Satisfies::VersionMismatch`.
  - Else: `Satisfies::Ready`.

`ManifestStore` is a trait you define; the production impl queries Buzz's
event store, the test impl is in-memory. This is the only place Almanac
*reads* lineage state.

**Verify:** unit tests against an in-memory `ManifestStore` covering:
ready, missing, version-mismatch, freshness-window-expired, multiple
contracts mixed.

**Commit:** `feat(almanac): lineage check_inputs against ManifestStore`

---

#### T11. Lineage graph (optional — skip if blocked)

**Spec:** In `crates/almanac-bridge/src/lineage/graph.rs`:

- `pub fn derive_edges(schedules: &[Schedule], contracts: &[Contract]) -> Vec<Edge>`
  where `Edge { consumer: String, producer: String, schema_id: String }`.
  Derives from contracts which schedules produce vs consume which schemas.

**Skip rule:** If T10 took longer than expected or this is proving
fiddly, write `// TODO: derive_edges for web view (Phase 3)` and move
to T12. The graph is only consumed by the Phase 3 web view; the ICS
feed doesn't need it.

**Verify:** one unit test with two schedules (producer + consumer)
producing the expected edge.

**Commit:** `feat(almanac): derive producer↔consumer edges from contracts`
_(or skip with a TODO comment and no commit)_

---

### Track E — HTTP surface

#### T12. Calendar HTTP route

**Spec:** In `crates/buzz-relay/src/http/` (or wherever routes live —
**verify the existing layout first**), add an `almanac` module:

- `GET /calendar/<community>.ics` — community-scoped, NIP-42 auth
  (same as every other community-scoped endpoint). Returns
  `text/calendar` content type. Renders all schedules in the community
  the subscriber can see, with today's run state overlaid.
- `GET /calendar/<community>/runs.ics` — one-off events per concrete
  run.
- `GET /calendar/<community>/schedule.ics` — recurring events only
  (no run overlay). This is the default if `.ics` is hit without a
  sub-path.

Wire it into the relay's router exactly as `buzz-search` does for its
HTTP routes. Copy that pattern; do not invent a new one.

**ACL:** per `10_PLAN.md` resolved decision #5 — omit schedules whose
channel is private to a pubkey set the requester isn't in.

**Verify:**
```bash
# start a local relay per buzz's dev setup (just relay)
# emit a KIND_WORKFLOW_DEF event
# then:
curl -sS http://localhost:3000/calendar/<community>.ics | head -20
# paste output into https://icalendar.org/validator.html — must be valid
```

**Commit:** `feat(relay): add /calendar/<community>.ics ICS endpoint`

---

### Track F — CLI

#### T13. `buzz almanac` subcommand

**Spec:** In `crates/buzz-cli/src/commands/almanac.rs`:

- `buzz almanac subscribe [--community <id>]` — prints the ICS URL for
  the community. Default community from `BUZZ_AUTH_TAG` / config.
- `buzz almanac check <schedule-id>` — queries the relay for the
  schedule's contracts and prints lineage state (✅/❌/⚠️ per input).
- `buzz almanac declare --schedule <id> --role produce|consume
  --schema <id> [--min-version N] [--freshness S]` — writes a
  `KIND_ALMANAC_CONTRACT` event.

Match the existing CLI patterns (see AGENTS.md § "Agent CLI" and the
`buzz-cli/TESTING.md` runbook). The `--format compact` global flag
applies.

**Verify:** `cargo build --release -p buzz-cli`; then manually:
```bash
buzz almanac subscribe
buzz almanac declare --schedule daily-brief --role produce --schema research-brief
buzz almanac check daily-brief
```

**Commit:** `feat(cli): add buzz almanac subcommand`

---

### Track G — Docs + smoke

#### T14. README

**Spec:** `crates/almanac-bridge/README.md`. Must include:

- What Almanac is (one paragraph from `00_OVERVIEW.md`).
- **The latency disclaimer prominently:** Google Calendar polls every
  12–24h (sometimes up to 5 days). Apple Calendar ~1h. For real-time,
  use CalDAV (Phase 2).
- The subscribe URL format.
- The emoji legend (🟡 pending, ⏳ running, ✅ succeeded, ❌ failed,
  ⏸ skipped).
- The CATEGORIES → color mapping suggestion for Google Calendar.
- A link to `~/PROJECTS/almanac/docs/00_OVERVIEW.md` for the full
  vision.

**Verify:** proofread; ensure the latency section is at the top, not
buried.

**Commit:** `docs(almanac): README with latency disclaimer and subscribe guide`

---

#### T15. End-to-end smoke test

**Spec:** `crates/almanac-bridge/tests/feed_smoke.rs`. Stands up a test
relay (copy the pattern from `crates/buzz-test-client/tests/e2e_*`),
then:

1. Emit a `KIND_WORKFLOW_DEF` (daily cron).
2. Emit a `KIND_ALMANAC_CONTRACT` (the schedule produces
   `research-brief`).
3. Emit a second `KIND_WORKFLOW_DEF` (weekly cron) that consumes
   `research-brief`.
4. Trigger the daily schedule's run; emit a `KIND_ALMANAC_MANIFEST`.
5. `GET /calendar/<community>.ics`. Parse with `icalendar` crate.
6. Assert:
   - Both schedules present as VEVENTs.
   - The daily one has `STATUS:CONFIRMED` (succeeded) and a ✅ emoji.
   - The weekly one has `RELATED-TO;RELTYPE=DEPENDS-ON` pointing at
     the daily schedule's UID.
   - The weekly one's `DESCRIPTION` contains a ✅ marker for the
     consumed artifact.

**Verify:** `cargo test -p almanac-bridge --test feed_smoke`
(requires Postgres + Redis — `just test` context).

**Commit:** `test(almanac): end-to-end feed smoke test`

---

## Done condition

Phase 1 is complete when **all** of these hold:

- [ ] T1–T15 committed on `feat/almanac-phase-1` (T11 may be skipped per
      its skip rule).
- [ ] `cargo test --workspace` green (or only blocked-by-infra tests
      skipped, with the skip noted in `BLOCKERS.md`).
- [ ] `cargo clippy --workspace -- -D warnings` green.
- [ ] `cargo fmt --all --check` green.
- [ ] `BLOCKERS.md` is empty or contains only documented, non-blocking
      issues.
- [ ] `RUN_LOG.md` lists every task with a commit SHA.
- [ ] The smoke test (`feed_smoke.rs`) passes against a local relay.

When all boxes are ticked, append to `RUN_LOG.md`:
`PHASE 1 COMPLETE at <ISO timestamp>`.
**Do not push. Do not open a PR.** Stop and report.

---

## Blocker protocol

When a task cannot be completed:

1. **Revert to the last green commit** for the affected files:
   ```bash
   git restore crates/...
   ```
   (Or `git switch -` to discard the working tree for the task.)

2. **Write to `~/PROJECTS/almanac/BLOCKERS.md`:**
   ```
   [T<n>] [<ISO timestamp>]
   Symptom: <what you observed, with the exact error message>
   Attempted: <what you tried>
   Decision: <either "implemented workaround X" or "blocked on Y —
             cannot proceed without the architect; stopped">
   Next: <the next non-blocked task to proceed to>
   ```

3. **Pick the next non-blocked task** from the list. Do not stop the run
   just because one task is blocked — most blockers are local. The only
   blocker types that justify stopping the entire run are:
   - T1 (kinds) — everything depends on it.
   - T2 (crate scaffold) — everything depends on it.
   - A workspace-wide compilation break not caused by your changes.

4. **If you hit 3 blockers in a row** that you can't route around, stop.
   Append `RUN HALTED: 3 consecutive blockers` to `RUN_LOG.md` and
   report. Continuing past 3 blockers usually means you're fighting a
   deeper issue.

---

## Long-context discipline

This run will exceed one context window. Mitigations:

1. **Append to `RUN_LOG.md` after every task.** The log plus this file
   is enough to resume after a context summarization. Format:
   `[T<n>] [done|blocked|skipped] [<commit-sha|blocker-ref>]`.

2. **Re-orient at the start of every task** (cheap; do it):
   ```bash
   cat ~/PROJECTS/almanac/RUN_LOG.md
   cat ~/PROJECTS/almanac/BLOCKERS.md
   git log --oneline -10
   ```

3. **Delegate exploration to subagents.** When a task requires reading
   existing code you don't know (e.g., "how does `buzz-search`
   register its routes?"), spawn an Explore subagent with a focused
   question. Keep the main context for decisions and commits.

4. **Don't re-read the whole plan every task.** Re-read only the
   section for the current task plus the "Operating principles" at the
   top of this file. The full plan is for orientation, not for
   per-task reference.

5. **Stop in a clean state.** Every task ends either committed or
   reverted. Never leave a half-implemented change in the working tree
   when you move to the next task or when the context is about to
   summarize.

---

## What success looks like

A clean `feat/almanac-phase-1` branch with 13–15 focused commits, all
tests green, the smoke test demonstrating the full dependency-checkmark
flow end-to-end, and a `RUN_LOG.md` that tells the story of what got
done. That's a shippable Phase 1.

If only a subset is done — say T1–T9, with the HTTP and CLI surfaces
blocked — that's still real progress: the data model and rendering
layers are the IP; the HTTP and CLI are mechanical wiring that any
follow-up session can finish. The run log + blocker log make the
remaining work obvious to a follow-up context.

**A clean partial result beats a sloppy full result.** Verify or stop.
