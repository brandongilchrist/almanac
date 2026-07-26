# Almanac — Agent-native integration

Almanac is **agent-native**: any MCP-capable client (Claude Desktop, ZCode,
Goose, the MCP Inspector) drives it as a first-class citizen. Agents register
themselves, own schedules, declare contracts, and check lineage — all over the
Model Context Protocol.

## What "agent-native" means here

| Capability | How |
|---|---|
| **Agents are principals** | An agent registers once (`register_agent`) with a name + kind; every schedule and run it creates is attributed to it. |
| **Agents drive the calendar** | `create_schedule`, `declare_contract`, `record_manifest`, `trigger_run` — all MCP tools. No terminal, no API key juggling. |
| **The dependency graph is agent-readable** | `check_lineage` returns ✅/❌/⚠️ per declared input — an agent asks "are my inputs ready?" and gets a structured answer. |
| **The visual surface reflects agents** | The dashboard at `/` shows an Agents panel grouping schedules by owner. |

This is the lineage engine (the agent dependency graph) made first-class: the
thing Palantir Foundry does for data pipelines, but for agent artifacts, and
driven by the agents themselves.

## Connecting an MCP client

`almanac-mcp` speaks MCP over **stdio**. Point any MCP client at the binary.

### Claude Desktop (`claude_desktop_config.json`)

```jsonc
// macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "almanac": {
      "command": "almanac-mcp",
      "env": {
        "ALMANAC_MCP_SEED": "1",       // seed demo data so you can explore
        "ALMANAC_COMMUNITY": "demo"    // default community
      }
    }
  }
}
```

Restart Claude Desktop. You'll see the Almanac tools available; ask Claude:
*"Use Almanac to show me the dependency graph for the demo community"* and it
will call `get_calendar` and render the DAG.

### ZCode (`~/.zcode/mcp.json` or workspace `.mcp.json`)

```jsonc
{
  "mcpServers": {
    "almanac": {
      "command": "almanac-mcp",
      "args": [],
      "env": { "ALMANAC_MCP_SEED": "1" }
    }
  }
}
```

### The MCP Inspector (no client needed)

```bash
npx @modelcontextprotocol/inspector almanac-mcp
```

Opens a browser UI listing every tool with its JSON schema — the fastest way
to explore the surface.

## The 10 tools

| Tool | Purpose |
|---|---|
| `register_agent` | Register or update an agent identity (name, kind, avatar). |
| `create_schedule` | Create/update a recurring schedule; attribute to an owner agent. |
| `list_schedules` | List schedules in a community with latest run status. |
| `declare_contract` | Declare a produce/consume edge in the dependency graph. |
| `record_manifest` | Record that a run materialized an artifact (the lineage primitive). |
| `check_lineage` | Check a schedule's inputs: ✅ ready / ❌ missing / ⚠️ version mismatch. |
| `trigger_run` | Create a run (Pending state). |
| `update_run_status` | Advance a run: running → succeeded / failed / skipped. |
| `get_calendar` | Full calendar state as JSON (agents, schedules, contracts, lineage). |

Field-level docs become the JSON input schema automatically; an MCP client
sees the full schema for each tool.

## A complete agent flow

An agent that produces a daily brief and depends on nothing:

```
1. register_agent(agent_id="research-bot", name="Research Bot", kind="cron")
2. create_schedule(schedule_id="daily-brief", summary="Daily research brief",
                   rrule="FREQ=DAILY;BYHOUR=9", owner_agent_id="research-bot")
3. declare_contract(schedule="daily-brief", role="produce", schema="research-brief")
4. trigger_run(schedule_id="daily-brief")           → returns run-id
5. update_run_status(run_id=..., status="succeeded")
6. record_manifest(run_id=..., schema_id="research-brief",
                   schema_version=1, content_hash="sha256:...",
                   uri="https://github.com/org/repo/commit/abc123")
```

A downstream agent that consumes the brief:

```
1. register_agent(agent_id="strategist", ...)
2. create_schedule(schedule_id="weekly-strategy", ...)
3. declare_contract(schedule="weekly-strategy", role="consume",
                    schema="research-brief", min_version=1, freshness=604800)
4. check_lineage(schedule_id="weekly-strategy")
   → "✅ ready (v1)  research-brief"   (because step 6 above recorded a manifest)
```

## Standalone vs. shared state

`almanac-mcp` ships as a standalone binary that owns its own in-memory state
and seeds the demo community (`ALMANAC_MCP_SEED=1`). This is the right default
for local agent exploration.

For a shared deployment (multiple agents, persistence, the visual dashboard),
run `almanac-server` and point agents at it via HTTP instead — the MCP server
and the HTTP server speak to the same data model. A future release will let
`almanac-mcp` proxy to a running server so MCP + HTTP share one state.

## The ICS export (still first-class)

The agent-native surface is the product; the ICS feed is the export. Every
community is also reachable at `/calendar/<community>.ics` so the same data
lands in Google/Apple/Outlook. The latency caveats from the README apply —
agents get live lineage via MCP; calendar clients poll.

## See also

- [`00_OVERVIEW.md`](00_OVERVIEW.md) — what Almanac is and is not.
- [`10_PLAN.md`](10_PLAN.md) — data model, kinds, ICS contract.
- The dashboard: run `almanac serve` → `http://localhost:8787`.
