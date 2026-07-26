# Almanac — Meta-Plan (How to Actually Build This)

This doc assumes you'll drive the work via the **goal slash-command** workflow
(break the project into goal-sized chunks, execute each against a focused
context, verify, commit, repeat). It's the operating manual for *building*
Almanac, not for *what* Almanac is.

Read `00_OVERVIEW.md` (the why) and `10_PLAN.md` (the what) first. This is
the how-to-execute.

---

## Why a meta-plan

Almanac is too big for one context window. It will touch `buzz-core` (new
kinds), a new crate (`almanac-bridge`), `buzz-relay` (HTTP routes + event
observation), `buzz-workflow` (Phase 2 gating hook), and `buzz-cli` (CLI
subcommands). Doing this in one streaming session produces drift, missed
conformance details, and "I forgot to run the ICS validator" bugs.

The discipline:

1. **Every goal is small enough to verify in isolation.** If you can't
   describe the exit criterion in one sentence, the goal is too big.
2. **Every goal ends with a green check or a written rollback.** No "I'll
   finish the tests next time."
3. **Every goal that touches kinds or wire formats updates the docs first.**
   The plan docs are the source of truth; code follows.
4. **Long context is managed, not suffered.** Use sub-agents for exploration,
   keep the main context for decisions and commits.

---

## Repository setup

Almanac builds *inside* the Buzz repo. It is not a standalone project — it
plugs into `buzz-relay` like the other crates.

```
~/PROJECTS/BUZZ/                      # the existing repo
  crates/almanac-bridge/              # NEW — Phase 1 deliverable
  crates/buzz-core/src/kind.rs        # MODIFY — add KIND_ALMANAC_*
  crates/buzz-relay/src/http/         # MODIFY — register /calendar routes
  crates/buzz-cli/src/commands/       # MODIFY — add almanac subcommand

~/PROJECTS/almanac/                   # this planning repo
  docs/
    00_OVERVIEW.md
    10_PLAN.md
    20_META_PLAN.md
  GOALS.md                            # working ledger (already exists)
```

The `~/PROJECTS/almanac/` directory is for planning artifacts only. Code
lives in the Buzz repo. This keeps planning state out of the codebase
and lets you re-orient across context windows by reading these docs.

---

## Working in Buzz (matters for every goal)

Before any goal that runs `cargo` or `git`, activate the Hermit
toolchain. From AGENTS.md:

```bash
. ./bin/activate-hermit   # from the repo root
```

This sets up Rust, Node, and everything else. Don't rewrite hook commands
to work around a missing toolchain — activate Hermit.

**Quality gate before every commit / PR:**

```bash
just ci      # fmt + clippy + desktop lint + unit tests + builds
just test    # full integration suite (needs Postgres + Redis)
```

Pre-commit hooks auto-fix formatting. Pre-push hooks run clippy. Builds
are CI-only. Run `just fix-all` if fmt drifts.

---

## The goal-decomposition strategy

### Goal size: "one verification cycle"

A goal is the right size if:

- It can be described in 1–2 sentences.
- It has **one** exit criterion that's objectively checkable (a passing
  test, a green CI run, a feed that validates).
- It produces one commit (or a small, related cluster of commits).
- It doesn't span more than ~3 files in more than ~2 crates, unless those
  files are tightly coupled (e.g., adding a kind in `buzz-core` and its
  handler in `buzz-relay` is one goal).

Too big: "Implement the ICS bridge." Too small: "Add a doc comment."
Right-sized: "Add `KIND_ALMANAC_SCHEDULE` to `buzz-core`, with a unit test
asserting it's parameterized-replaceable in the 48050–48099 range."

### Goal sequencing: data model before code, always

For each layer, in this order:

1. **Decide the shape** (update `10_PLAN.md` if needed).
2. **Add the kind / type / event** with tests.
3. **Add the handler that reads it.**
4. **Add the renderer / output that consumes it.**
5. **Wire the HTTP / CLI surface.**
6. **End-to-end smoke test.**

Skipping step 1 produces code that has to be rewritten when the data model
settles. Always do step 1 in the doc first.

---

## The goal backlog (Phase 1)

This is the suggested breakdown. Each line is one goal. Work top-to-bottom;
each goal assumes the ones above it are merged.

### Track A — Data model (do first; everything depends on it)

1. **Allocate kind range.** Add `KIND_ALMANAC_SCHEDULE` (48050),
   `KIND_ALMANAC_RUN` (48051), `KIND_ALMANAC_MANIFEST` (48052),
   `KIND_ALMANAC_CONTRACT` (48053), `KIND_ALMANAC_CALENDAR` (48054) to
   `buzz-core/src/kind.rs`. Add `is_almanac_kind()` helper. Unit tests
   assert each is parameterized-replaceable and in range. Exit: `cargo test
   -p buzz-core kind::almanac` green.

2. **_(removed — data-model decisions are made in `10_PLAN.md` § "Decided
   data-model decisions". Nothing to resolve.)_** Skip straight to G3.

3. **Define Rust structs.** In `crates/almanac-bridge/src/model.rs`,
   define `Schedule`, `Run`, `Manifest`, `Contract`, `Calendar`, with serde
   round-trip tests. Pure data; no I/O. Exit: `cargo test -p almanac-bridge
   model` green.

### Track B — Ingestion (events → structs)

4. **Schedule ingestion.** Translate `KIND_WORKFLOW_DEF` (30620) events
   into `Schedule` structs. Unit test with a fixture event. Exit: parse
   test green; builder round-trips.

5. **Run ingestion.** Translate observed fire-claim events (from
   `buzz-workflow`'s durable fire store) + agent output events into `Run`
   structs with status. Exit: unit tests cover each status transition.

6. **Manifest emission.** Best-effort: parse an agent's output event for
   artifact references (GitHub commit URL, file path). Emit
   `KIND_ALMANAC_MANIFEST`. If nothing parseable, point manifest at the
   agent's output thread. Exit: a real agent run produces a manifest event
   visible via `buzz query`.

### Track C — Rendering (structs → ICS)

7. **VEVENT rendering — recurring schedule.** Use the `icalendar` + `rrule`
   crates to render a `Schedule` into a `VEVENT` with `RRULE`. Unit test
   asserts the output parses and contains the expected fields. Exit:
   validator-green ICS for a sample schedule.

8. **VEVENT rendering — status overlay.** Render today's `Run` state into
   the event's `STATUS` + emoji-prefixed `SUMMARY`. Unit test per status
   value. Exit: every status in the table has a test.

9. **Lineage rendering.** For a schedule with `consume` contracts, emit
   `RELATED-TO;RELTYPE=DEPENDS-ON` and append the check state to
   `DESCRIPTION`. Unit tests: input present (✅), missing (✗), version
   mismatch (⚠️). Exit: rendering tests green.

### Track D — HTTP surface

10. **Calendar route.** Register `GET /calendar/<community>.ics` on the
    relay's router, community-scoped, NIP-42 auth. Returns the rendered
    ICS. Exit: `curl` returns a validator-green feed.

11. **Split feeds.** Add `/runs.ics` (concrete runs) alongside the default
    `/schedule.ics`. Exit: two feeds, each validator-green, each scoped.

### Track E — CLI + docs

12. **`buzz almanac` subcommand.** `subscribe` (prints URL), `check
    <schedule>` (prints lineage state), `declare` (writes a contract event).
    Exit: each subcommand works against a local relay.

13. **README + latency disclaimer.** Document the Google 12-24h lag, the
    Apple ~1h lag, the CalDAV upgrade path. Exit: README merged.

14. **End-to-end smoke test.** Add `crates/almanac-bridge/tests/feed_smoke.rs`
    that stands up a test relay, emits events, hits `/calendar.ics`, parses
    the result, and asserts on content. Exit: test green in `just test`.

**Phase 1 done when:** goals 1–14 are merged, `just ci` is green, and a
schedule manually triggered in a local relay shows up in Apple Calendar with
a status flip within an hour.

---

## Per-goal workflow template

For every goal, follow this loop. Don't skip steps; the discipline is the
point.

```
1. ORIENT (read-only, cheap)
   - Read the relevant section of 00_OVERVIEW.md and 10_PLAN.md.
   - Read the relevant existing code (e.g., how buzz-search registers its
     HTTP routes — copy that pattern).
   - If exploring unfamiliar code, delegate to a subagent (Explore type)
     rather than burning main-context tokens.

2. DECIDE
   - If the goal touches the data model, update 10_PLAN.md first.
   - Write the exit criterion as a one-liner at the top of your goal note.

3. IMPLEMENT
   - Make the change. Match surrounding code style (Buzz has strong
     conventions — see AGENTS.md § Key Patterns).
   - No new unwrap()/expect() in production paths. Use ? and proper errors.
   - Doc comments on every new public API.

4. VERIFY
   - Run the relevant slice of `just ci`. For Rust-only changes:
     `cargo fmt && cargo clippy -p <crate> && cargo test -p <crate>`.
   - For ICS output: paste into icalendar.org/validator.html.
   - Don't commit until the exit criterion is objectively met.

5. COMMIT
   - One commit per goal (or a tight cluster if they don't make sense apart).
   - Commit message format matches recent buzz history (see `git log
     --oneline -20`): `feat(almanac): …`, `fix(almanac): …`,
     `docs(almanac): …`.
   - Don't push until asked. Don't open a PR until asked.

6. UPDATE THE BACKLOG
   - Mark the goal done in GOALS.md.
   - Note any follow-ups discovered during the goal (these become new goals).
```

---

## Long-context management

This project will exceed a single context window. Plan for it.

### Keep a GOALS.md ledger

In `~/PROJECTS/almanac/GOALS.md`, maintain a single flat list:

```markdown
# Almanac — Goal Ledger

## Done
- [x] G1: Allocate kind range (48050–48054). Commit abc1234.
- [x] G2: _(removed — data-model decisions live in `10_PLAN.md` § "Decided data-model decisions"; no separate step.)_

## In Progress
- [ ] G3: Define Rust structs in model.rs.

## Next
- [ ] G4: Schedule ingestion.
- [ ] G5: Run ingestion.

## Discovered (park; promote to Next when ready)
- G3 follow-up: serde round-trip test for `Manifest` found a nullable field
  bug — filed as G3b.
```

When a context summary happens, the summary plus this file is enough to
re-orient. Don't rely on memory; rely on the ledger.

### Delegate exploration to subagents

When a goal requires understanding existing code (e.g., "how does
`buzz-search` register its routes?"), don't read every file in the main
context. Spawn an Explore subagent with a focused question, get the
conclusion, and act on it. This keeps the main context for decisions and
commits, not for file dumps.

### Re-orient at the start of every goal

Before starting work on a new goal, in this order:

1. Read `GOALS.md` — what's done, what's next.
2. Read the relevant section of `10_PLAN.md` — the spec for this layer.
3. `git log --oneline -5` — what just changed.
4. Then start.

This takes 60 seconds and prevents the most common failure mode: building
the wrong thing because you forgot what was already merged.

---

## Branch and commit strategy

- **Branch per phase.** `feat/almanac-phase-1`, `feat/almanac-phase-2`.
- Within a phase, commit per goal. Squash-merge at the end of the phase if
  you want a clean history, or keep the goal-by-goal history if reviewers
  prefer it.
- **Never push or open a PR unless explicitly asked.** This is in the
  global agent guidance; it applies here.
- Run `just ci` before any push. CI will run it again, but local green
  saves a round-trip.

---

## What to do when you get stuck

1. **Spec ambiguity.** Re-read the relevant section of `10_PLAN.md`. If
   it's ambiguous, the answer is "update the doc to remove the ambiguity,"
   not "guess." Open questions go in the Open Questions section and get
   resolved deliberately, not in passing.

2. **Buzz-convention question** (e.g., "how do other crates register HTTP
   routes?"). Spawn an Explore subagent. Don't guess from memory;
   conventions drift.

3. **ICS conformance failure.** Paste the feed into
   [icalendar.org/validator.html](https://icalendar.org/validator.html)
   *first*, before debugging. 90% of "works in Google, breaks in Apple"
   bugs are caught here.

4. **`buzz-workflow` event shape changed under you.** Add a contract test
   that asserts the shape you depend on. If it breaks, that's a signal,
   not a nuisance — pin to a version or update the parser deliberately.

5. **Temptation to build the web UI.** Re-read `00_OVERVIEW.md` § "What
   Almanac is not." The web UI is Phase 3 at the earliest, and even then
   it's server-rendered. If you're writing React, stop.

6. **Temptation to reinvent scheduling.** Re-read `10_PLAN.md` § "Out of
   scope." `buzz-workflow` already has a cron engine. Almanac observes;
   it does not schedule.

---

## Definition of done — Phase 1

Phase 1 is shippable when **all** of these are true:

- [ ] All 14 goals in the backlog are merged.
- [ ] `just ci` is green at the tip of `feat/almanac-phase-1`.
- [ ] `just test` passes (including the new `feed_smoke.rs`).
- [ ] A local end-to-end demo works: emit a schedule, trigger it, see the
      event appear in Apple Calendar (or Google Calendar, with the lag
      caveat), see the status flip after the run completes.
- [ ] A schedule with a declared input contract shows the green-check /
      red-X state correctly.
- [ ] `README.md` documents the latency limits honestly.
- [ ] `GOALS.md` is up to date; Phase 2 goals are drafted in "Next."

When those boxes are ticked, Phase 1 is done. Phase 2 (CalDAV + gating) is
a separate effort with its own branch and its own definition of done.

---

## One last discipline

The most common way this project fails isn't technical. It's scope creep —
Almanac slowly absorbing the roles of `buzz-workflow`, `buzz-acp`, or
becoming a general-purpose calendar web app. The overview's "What Almanac
is not" section is the contract.

**Re-read it at the start of every goal session.** If a goal seems to
require crossing one of those lines, the answer is almost always "that's a
different project" or "that's a later phase," not "let's expand Almanac's
scope."
