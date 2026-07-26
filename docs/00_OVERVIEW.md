# Almanac — Overview

> A calendar for agents and their artifacts.
> Subscribe once in any calendar app; every scheduled agent job, the artifact
> it produces, and the artifacts it depends on show up as events — with green
> checks when inputs are ready, red marks when they're not.

---

## What it is

**Almanac** is a thin rendering layer over the existing **Buzz** platform that
turns scheduled agent work into **standard iCalendar events** (`RFC 5545` /
`RFC 7986` / `RFC 9253`). It is **not** a new calendar UI, **not** a new cron
engine, and **not** a new agent runtime. It is the missing **planning + lineage
view** for agent work that already happens in Buzz.

Almanac serves a single URL — `GET /calendar/<community>.ics` — that any
calendar app on earth can subscribe to: Google Calendar, Apple Calendar,
Outlook, DAVx⁵, Thunderbird, a wall display, an Apple Watch. No app to install.

## The problem it solves

Today, if you run a fleet of scheduled agents (research briefs daily, a
strategy draft weekly, a code review on every PR merge, …), you have no way
to answer these questions at a glance:

1. **What's scheduled to run, and when?**
2. **For each job, what does it need as input — and is that input ready?**
3. **What did each job produce — and did it succeed?**
4. **What depends on what?** (If Monday's brief fails, what Friday jobs are
   now blocked?)

Buzz already runs the agents and stores the events. It just has no calendar
*view*, and no concept of **artifact lineage** — the dependency-checkmark
relationship between "this job needs that artifact."

## The wedge: dependency checkmarks

The novel idea is small but nobody does it well:

> Each scheduled agent job declares its **inputs** (artifacts it consumes)
> and its **outputs** (artifacts it produces). When a job finishes, Almanac
> records a manifest for what it produced — content hash, schema version,
> commit SHA, timestamp. When a downstream job is about to run, it checks:
> *does a materialized manifest exist for each of my inputs?* If yes, the
> calendar event shows a green check. If no, it shows a red ✗ and the job
> refuses to start (or runs in degraded mode you define).

This is **data lineage** (the thing Palantir Foundry and Dagster do for data
pipelines) applied to **agent artifacts**. It's encoded as `RELATED-TO;
RELTYPE=DEPENDS-ON` (RFC 9253) so the relationship survives every calendar
sync, even if no client currently renders it visually — the *state* lives in
`STATUS` + `CATEGORIES` fields that every client does render.

## Where it lives in the stack

```
┌─────────────────────────────────────────────────────────────────┐
│  ANY CALENDAR CLIENT (Google / Apple / Outlook / DAVx⁵ / …)     │
│  Subscribes to one URL. Just renders events. No install.        │
└────────────────────────────────────────────────────────────────┘
                            ↑ subscribes (one-way ICS feed)
┌─────────────────────────────────────────────────────────────────┐
│  ALMANAC ICS BRIDGE       (Rust, runs in the Buzz relay process)│
│  - Reads cron defs + artifact manifests from Buzz events        │
│  - Renders to RFC 5545/7986/9253 iCalendar                      │
│  - Encodes lineage state into STATUS / CATEGORIES / SUMMARY     │
│  - Serves /calendar/<community>.ics (and /runs.ics, /deps.ics)  │
└─────────────────────────────────────────────────────────────────┘
                            ↑ reads existing Buzz events
┌─────────────────────────────────────────────────────────────────┐
│  BUZZ (already exists)                                           │
│  - buzz-workflow: cron engine with window-based tick matching    │
│  - buzz-acp:     agent subprocess harness (default agent: goose) │
│  - buzz-relay-mesh: pooled community compute (mesh-llm)         │
│  - Nostr events: channels, threads, agent identities, auth      │
└─────────────────────────────────────────────────────────────────┘
```

**Almanac does not own the schedule, the agent, or the compute.** It only owns
the rendering of those things into a calendar-shaped view, plus the small
amount of lineage state needed to make checkmarks work.

## What Almanac is not

To prevent scope creep, the things Almanac deliberately does **not** do:

- **Not a calendar UI.** No web app, no React, no FullCalendar. The view is
  whatever calendar the user already uses.
- **Not a cron engine.** `buzz-workflow` already has one. Almanac subscribes
  to its events; it does not replace it.
- **Not an agent runtime.** `buzz-acp` + Goose already handle this. Almanac
  observes; it does not execute.
- **Not a new protocol.** iCalendar, Nostr, and Buzz event kinds are all that
  is used. No new wire format.
- **Not real-time by default.** Phase 1 ICS feeds poll on the client's
  schedule (Google: 12-24h, Apple: ~1h). Real-time is a Phase 2 CalDAV
  concern.
- **Not the source of truth.** Buzz is. Almanac is a derived view; if
  Almanac crashes, agents keep running. The calendar just stops updating.

## The three documents

This overview is one of three. Read in this order:

1. **`00_OVERVIEW.md`** (this file) — *what* and *why*. Read first.
2. **`10_PLAN.md`** — *how*. Concrete phases, kind numbers, file layout,
   data model, integration points. The implementation spec.
3. **`20_META_PLAN.md`** — *how to actually build it*. The workflow for
   using the goal/slash-command workflow against this repo, including how
   to break the plan into goal-sized chunks, what to verify at each step,
   and how to manage the long context this work will generate.

## Why this design (one paragraph)

iCalendar is a 30-year-old, universally supported open standard. Building a
custom web calendar would lock users into one more app they have to open.
Rendering the same information as standard events — using `RRULE` for the
schedule, `RELATED-TO;RELTYPE=DEPENDS-ON` for dependencies (RFC 9253), and
`STATUS`/`CATEGORIES` for state — gives you universal compatibility on day
one, with a clean upgrade path to CalDAV (real-time push) for power users
later. The hard IP is the **data model** (artifact manifests + lineage), not
the UI. Solving that well and rendering it through a standard protocol is the
whole product.

## Naming

- **Almanac** — the project. An almanac is "a calendar plus a record of what
  happens" — exactly the predicted-schedule-plus-recorded-outcomes data model.
- **Manifest** — a record of an artifact's materialization (one job's output).
- **Lineage** — the dependency graph between artifacts (consumers → producers).
- **Run** — one execution of a cron job (concrete; produces a manifest).
- **Schedule** — the recurring cron definition itself (the plan).

Internal Rust names follow Buzz conventions: `KIND_ALMANAC_*` constants,
`almanac-*` crate names, `almanac::` module paths.
