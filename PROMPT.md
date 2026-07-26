# Almanac — The One Prompt

Paste everything below this line into a fresh Claude Code (or goose, or
whatever agent) session, in the `~/PROJECTS/BUZZ` repo. Then walk away.

---

You are running an unattended overnight build of **Almanac Phase 1** — a
calendar + artifact-lineage rendering layer for the Buzz platform.

Your entire job is to execute the spec at
`~/PROJECTS/almanac/docs/30_AUTONOMOUS_EXECUTION.md` faithfully, from
start to finish, without human input. You will not ask questions. You
will not request clarification. Every decision you need is already made
in the docs.

## Read these in order, then begin

1. `~/PROJECTS/almanac/docs/00_OVERVIEW.md` — what Almanac is and is not.
   **Re-read the "What Almanac is not" section before every task.**
2. `~/PROJECTS/almanac/docs/10_PLAN.md` — data model, kinds, ICS contract,
   phases. The "Resolved data-model decisions" section is authoritative.
3. `~/PROJECTS/almanac/docs/20_META_PLAN.md` — conventions, working-in-Buzz
   rules, the goal-decomposition rationale.
4. `~/PROJECTS/almanac/docs/30_AUTONOMOUS_EXECUTION.md` — **the execution
   loop you will follow.** This is your primary instruction.

## The work

Execute tasks T1 through T15 in `30_AUTONOMOUS_EXECUTION.md`, strictly in
order. For each task: read its spec → implement → verify → commit → log
to `~/PROJECTS/almanac/RUN_LOG.md`. Do not start the next task until the
current one is committed or cleanly blocked-and-reverted per the
Blocker Protocol in the spec.

## Hard rules (non-negotiable)

1. **Never commit red.** If a test fails and you can't fix it within a
   reasonable attempt, revert to the last green state, log to
   `~/PROJECTS/almanac/BLOCKERS.md`, and move to the next non-blocked
   task. Do not "come back to it later" — either it's green or it's
   reverted.
2. **Never push, never open a PR.** Local commits on
   `feat/almanac-phase-1` only. The human will push when they review.
3. **Never exceed scope.** If a task seems to require building a web UI,
   replacing `buzz-workflow`, running agents, or adding a new protocol,
   you have drifted. Re-read `00_OVERVIEW.md` § "What Almanac is not"
   and stop.
4. **Never substitute your own architecture.** The data model, kind
   numbers (48050–48054), crate name (`almanac-bridge`), and ICS field
   mappings in `10_PLAN.md` are the spec. If you believe something is
   wrong, log it to `BLOCKERS.md` and continue with the documented
   decision.
5. **No new `unwrap()`/`expect()` in production paths.** Use `?` and
   proper error types. Match Buzz conventions (see AGENTS.md).
6. **Activate Hermit before any cargo/git work:**
   ```bash
   . ./bin/activate-hermit
   ```
   Do not rewrite hook commands to work around a missing toolchain.
7. **Run `just ci`'s components as you go** (fmt, clippy, tests for the
   crates you touch). Don't save quality gates for the end.

## Stopping conditions

Stop the run and report when **any** of these is true:

- All 15 tasks are done and the "Done condition" checklist in
  `30_AUTONOMOUS_EXECUTION.md` is fully ticked.
- You hit 3 blockers in a row that you cannot route around (append
  `RUN HALTED: 3 consecutive blockers` to `RUN_LOG.md` and stop).
- T1 or T2 is blocked (everything depends on them; nothing else can
  proceed).
- The workspace breaks in a way not caused by your changes (a `buzz-*`
  crate won't compile independent of your work).

Do **not** stop just because one task is blocked — most blockers are
local. Route around them per the Blocker Protocol and continue with the
next non-blocked task.

## Context management

This run will exceed one context window. Mitigations (also in the spec):

- Append one line to `~/PROJECTS/almanac/RUN_LOG.md` after every task:
  `[T<n>] [done|blocked|skipped] [<commit-sha|blocker-ref>]`.
- Before starting each task, re-orient:
  ```bash
  cat ~/PROJECTS/almanac/RUN_LOG.md
  cat ~/PROJECTS/almanac/BLOCKERS.md
  git log --oneline -10
  ```
- Delegate exploration ("how does `buzz-search` register routes?",
  "what's the shape of `KIND_WORKFLOW_DEF`?") to subagents. Keep the
  main context for decisions and commits.
- Stop in a clean state at every task boundary — committed or reverted,
  never half-implemented.

## When you finish (or stop)

Append a final summary to `~/PROJECTS/almanac/RUN_LOG.md`:

```
## Run summary
Started: <ISO>
Stopped: <ISO>
Tasks completed: T1, T2, …
Tasks blocked: T<n> — <one-line reason>
Phase 1 status: <complete | partial — X/15 tasks done>
Head commit: <sha>
```

Then print a short human-readable summary of what got done, what's
blocked, and what the obvious next steps are. Do not push. Do not open
a PR.

Begin with the Pre-flight checklist in `30_AUTONOMOUS_EXECUTION.md`.
