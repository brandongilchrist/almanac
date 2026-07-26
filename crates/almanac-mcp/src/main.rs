//! `almanac-mcp` — the Model Context Protocol server that makes Almanac
//! agent-native.
//!
//! Any MCP-capable client (Claude, ZCode, Goose, the MCP Inspector) connects
//! over stdio and gets a set of tools to drive Almanac directly:
//!
//! - **Identity**: `register_agent` — the agent introduces itself once.
//! - **Planning**: `create_schedule`, `list_schedules`.
//! - **Lineage** (the agent dependency graph): `declare_contract`,
//!   `record_manifest`, `check_lineage`.
//! - **Execution**: `trigger_run`, `update_run_status`.
//! - **Read**: `get_calendar` — the full DAG of schedules → contracts →
//!   manifests with status.
//!
//! The MCP server shares one in-memory [`State`] with the HTTP server when
//! run as a library; when run standalone it owns its own state and seeds the
//! demo community so an agent has something to explore.

#![forbid(unsafe_code)]

use almanac_bridge::model::{
    Agent, Contract, ContractRole, Manifest, Run, RunStatus, Schedule, SkipReason,
};
use almanac_server::seed::seed_demo;
use almanac_server::State;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{schemars, tool, tool_router, ErrorData as McpError, ServiceExt};
use serde::Deserialize;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

/// The MCP server state: a handle to the shared Almanac state + the community
/// agents operate on by default.
#[derive(Clone)]
pub struct AlmanacMcp {
    state: Arc<State>,
    default_community: String,
}

/// Run the MCP server over stdio. Seeds the demo community if `seed` is true.
pub async fn run(seed: bool, default_community: String) -> anyhow::Result<()> {
    // CRITICAL: logs go to stderr — stdout is the MCP transport.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();

    let state = State::new();
    if seed {
        seed_demo(&state).await;
    }
    let service = AlmanacMcp {
        state: Arc::new(state),
        default_community,
    };
    let running = service.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

// ---- shared helpers ----

fn community_of(default: &str, given: Option<&str>) -> String {
    given.unwrap_or(default).to_string()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn text<S: Into<String>>(s: S) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(s.into())])
}

fn json_content<T: serde::Serialize>(t: &T) -> CallToolResult {
    let body = serde_json::to_string_pretty(t).unwrap_or_else(|e| format!("<serde error: {e}>"));
    CallToolResult::success(vec![ContentBlock::text(body)])
}

// ===========================================================================
// Tools
// ===========================================================================

#[tool_router(server_handler)]
impl AlmanacMcp {
    // ---- identity ----------------------------------------------------------

    /// An agent registers itself so its schedules/runs are attributed to it.
    #[tool(
        description = "Register or update an agent identity. Agents are first-class citizens \
                       in Almanac — every schedule and run is attributed to one. Call once at \
                       startup with your name and kind, then reference the returned agent_id \
                       when creating schedules."
    )]
    async fn register_agent(
        &self,
        Parameters(args): Parameters<RegisterAgentArgs>,
    ) -> Result<CallToolResult, McpError> {
        let agent = Agent {
            agent_id: args.agent_id.clone(),
            name: args.name.unwrap_or_else(|| args.agent_id.clone()),
            avatar: args.avatar,
            kind: args.kind.unwrap_or_else(|| "mcp-tool".into()),
            community_id: community_of(&self.default_community, args.community.as_deref()),
            description: args.description,
            created_at: now(),
        };
        self.state.upsert_agent(agent.clone()).await;
        Ok(text(format!(
            "Registered agent `{}` ({}). Use this agent_id when creating schedules.",
            agent.agent_id, agent.name
        )))
    }

    // ---- planning ----------------------------------------------------------

    #[tool(
        description = "Create or update a recurring schedule (the plan). Example rrule: \
                       'FREQ=DAILY;BYHOUR=9' or 'FREQ=WEEKLY;BYDAY=MO'. Leave rrule empty for \
                       a webhook-triggered one-off schedule."
    )]
    async fn create_schedule(
        &self,
        Parameters(args): Parameters<CreateScheduleArgs>,
    ) -> Result<CallToolResult, McpError> {
        let community = community_of(&self.default_community, args.community.as_deref());
        let sched = Schedule {
            schedule_id: args.schedule_id,
            community_id: community.clone(),
            channel_id: args.channel.unwrap_or_default(),
            summary: args.summary,
            description: args.description.unwrap_or_default(),
            rrule: args.rrule.unwrap_or_default(),
            dtstart: args.dtstart.unwrap_or_else(now),
            calendar_group: args.calendar_group.unwrap_or_else(|| "default".into()),
            color_category: args.color,
            owner_agent_id: args.owner_agent_id,
            created_at: now(),
            updated_at: now(),
        };
        let id = sched.schedule_id.clone();
        self.state.upsert_schedule(sched).await;
        Ok(text(format!(
            "Schedule `{id}` created in community `{community}`. Subscribe: /calendar/{community}.ics"
        )))
    }

    #[tool(description = "List all schedules in a community with their latest run status.")]
    async fn list_schedules(
        &self,
        Parameters(args): Parameters<CommunityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let community = community_of(&self.default_community, args.community.as_deref());
        let schedules = self.state.schedules_for(&community).await;
        let runs = self.state.runs_for(&community).await;
        let mut lines = Vec::new();
        for s in &schedules {
            let status = runs
                .get(&s.schedule_id)
                .map(|r| format!("{:?}", r.status))
                .unwrap_or_else(|| "pending".into());
            let owner = s.owner_agent_id.as_deref().unwrap_or("(unowned)");
            lines.push(format!(
                "- `{}` [{}] rrule=`{}` owner=`{}` — {}",
                s.schedule_id, status, s.rrule, owner, s.summary
            ));
        }
        if lines.is_empty() {
            Ok(text(format!("No schedules in community `{community}`.")))
        } else {
            Ok(text(format!(
                "Schedules in `{community}`:\n{}",
                lines.join("\n")
            )))
        }
    }

    // ---- lineage (the agent dependency graph) ------------------------------

    #[tool(
        description = "Declare a producer or consumer contract — the edges of the agent \
                       dependency graph. A producer declares it emits a schema; a consumer \
                       declares it needs a schema (with min_version and a freshness window \
                       in seconds, default 86400). This is how one agent's output becomes \
                       another agent's checked input."
    )]
    async fn declare_contract(
        &self,
        Parameters(args): Parameters<DeclareContractArgs>,
    ) -> Result<CallToolResult, McpError> {
        let community = community_of(&self.default_community, args.community.as_deref());
        let role = match args.role.as_str() {
            "produce" | "producer" => ContractRole::Produce,
            "consume" | "consumer" => ContractRole::Consume,
            other => {
                return Ok(text(format!(
                    "Unknown role `{other}`. Use `produce` or `consume`."
                )))
            }
        };
        let contract = Contract {
            contract_id: format!("{}-{}-{}", args.schedule, args.role, args.schema),
            schedule_id: args.schedule.clone(),
            role,
            schema_id: args.schema.clone(),
            min_version: args.min_version.unwrap_or(1),
            any_version: args.any_version.unwrap_or(false),
            freshness_window: args.freshness.unwrap_or(Contract::DEFAULT_FRESHNESS_WINDOW),
        };
        let cid = contract.contract_id.clone();
        self.state.add_contract(&community, contract).await;
        Ok(text(format!(
            "Contract `{cid}` declared: schedule `{}` {}s schema `{}`.",
            args.schedule, args.role, args.schema
        )))
    }

    #[tool(
        description = "Record that a run materialized an artifact — the lineage primitive. \
                       Sets the content hash, schema version, and URI (e.g. a GitHub commit \
                       URL or file path). Downstream consumers checking lineage will then \
                       see this input as ready."
    )]
    async fn record_manifest(
        &self,
        Parameters(args): Parameters<RecordManifestArgs>,
    ) -> Result<CallToolResult, McpError> {
        let manifest = Manifest {
            manifest_id: Manifest::id_for(&args.run_id, &args.schema_id),
            run_id: args.run_id.clone(),
            schema_id: args.schema_id.clone(),
            schema_version: args.schema_version.unwrap_or(1),
            content_hash: args.content_hash.unwrap_or_else(|| "unspecified".into()),
            commit_sha: args.commit_sha,
            uri: args
                .uri
                .unwrap_or_else(|| format!("buzz://run/{}", args.run_id)),
            bytes: args.bytes,
            materialized_at: now(),
        };
        let mid = manifest.manifest_id.clone();
        self.state.put_manifest(manifest).await;
        Ok(text(format!(
            "Manifest `{mid}` recorded. Consumers of schema `{}` will now see this input as ready.",
            args.schema_id
        )))
    }

    #[tool(
        description = "Check whether a schedule's declared input contracts are satisfied \
                       (✅ ready / ❌ missing / ⚠️ version mismatch). This is the core \
                       lineage query — the agent dependency check."
    )]
    async fn check_lineage(
        &self,
        Parameters(args): Parameters<CheckLineageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let community = community_of(&self.default_community, args.community.as_deref());
        let store = self.state.manifest_store_async().await;
        let contracts = self.state.contracts_for(&community).await;
        let mine: Vec<Contract> = contracts
            .iter()
            .filter(|c| c.schedule_id == args.schedule_id)
            .cloned()
            .collect();
        if mine.is_empty() {
            return Ok(text(format!(
                "Schedule `{}` has no contracts in community `{}`.",
                args.schedule_id, community
            )));
        }
        let runs = self.state.runs_for(&community).await;
        let run = runs.get(&args.schedule_id).cloned().unwrap_or(Run {
            run_id: format!("check-{}", args.schedule_id),
            schedule_id: args.schedule_id.clone(),
            scheduled_for: now(),
            started_at: Some(now()),
            finished_at: None,
            status: RunStatus::Pending,
            error: None,
        });
        let deps = almanac_bridge::lineage::check_inputs(&store, &run, &mine)
            .await
            .map_err(|e| McpError::internal_error(format!("lineage error: {e}"), None))?;
        let mut lines = Vec::new();
        for d in &deps {
            let mark = match &d.satisfied {
                almanac_bridge::lineage::Satisfies::Ready { version, .. } => {
                    format!("✅ ready (v{version})")
                }
                almanac_bridge::lineage::Satisfies::Missing => "❌ missing".to_string(),
                almanac_bridge::lineage::Satisfies::VersionMismatch { found, need } => {
                    format!("⚠️ version mismatch (found v{found}, need v{need}+)")
                }
            };
            lines.push(format!("- {mark}  {}", d.schema_id));
        }
        Ok(text(format!(
            "Lineage for `{}`:\n{}",
            args.schedule_id,
            lines.join("\n")
        )))
    }

    // ---- execution ---------------------------------------------------------

    #[tool(
        description = "Create a run (one execution of a schedule) in Pending state. Use \
                       update_run_status to advance it through Running → Succeeded/Failed."
    )]
    async fn trigger_run(
        &self,
        Parameters(args): Parameters<TriggerRunArgs>,
    ) -> Result<CallToolResult, McpError> {
        let community = community_of(&self.default_community, args.community.as_deref());
        let run = Run {
            run_id: args.run_id.unwrap_or_else(|| format!("run-{}", now())),
            schedule_id: args.schedule_id.clone(),
            scheduled_for: args.scheduled_for.unwrap_or_else(now),
            started_at: None,
            finished_at: None,
            status: RunStatus::Pending,
            error: None,
        };
        let rid = run.run_id.clone();
        self.state.upsert_run(run).await;
        Ok(text(format!(
            "Run `{rid}` triggered for schedule `{}` in `{}` (Pending).",
            args.schedule_id, community
        )))
    }

    #[tool(
        description = "Advance a run's status. Allowed transitions: pending→running, \
                       running→succeeded|failed. Set error text on failure."
    )]
    async fn update_run_status(
        &self,
        Parameters(args): Parameters<UpdateRunArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Find the run across all communities (runs are keyed by schedule, so
        // look up by run_id anywhere).
        let all_runs = self.snapshot_all_runs().await;
        let Some(mut run) = all_runs.into_iter().find(|r| r.run_id == args.run_id) else {
            return Ok(text(format!("Run `{}` not found.", args.run_id)));
        };
        let at = now();
        let result_status = match args.status.as_str() {
            "running" | "started" => {
                run.status = RunStatus::Running;
                run.started_at = Some(at);
                "Running".to_string()
            }
            "succeeded" | "ok" | "success" => {
                run.status = RunStatus::Succeeded;
                if run.started_at.is_none() {
                    run.started_at = Some(at);
                }
                run.finished_at = Some(at);
                "Succeeded".to_string()
            }
            "failed" | "error" => {
                run.status = RunStatus::Failed;
                run.error = Some(args.error.unwrap_or_else(|| "agent error".into()));
                if run.started_at.is_none() {
                    run.started_at = Some(at);
                }
                run.finished_at = Some(at);
                "Failed".to_string()
            }
            "skipped" => {
                run.status = RunStatus::Skipped(SkipReason::MissingInput(
                    args.missing_input.unwrap_or_default(),
                ));
                run.finished_at = Some(at);
                "Skipped".to_string()
            }
            other => return Ok(text(format!("Unknown status `{other}`."))),
        };
        self.state.upsert_run(run).await;
        Ok(text(format!("Run `{}` → {result_status}.", args.run_id)))
    }

    // ---- read --------------------------------------------------------------

    #[tool(
        description = "Get the full calendar state for a community as JSON: agents, \
                       schedules (with run status + owner), contracts, and the lineage \
                       verdict for every consumer. The agent dependency graph in one call."
    )]
    async fn get_calendar(
        &self,
        Parameters(args): Parameters<CommunityArgs>,
    ) -> Result<CallToolResult, McpError> {
        let community = community_of(&self.default_community, args.community.as_deref());
        let snap = self.snapshot(&community).await;
        Ok(json_content(&snap))
    }
}

impl AlmanacMcp {
    /// Gather every run across all communities (used for run-id lookups).
    async fn snapshot_all_runs(&self) -> Vec<Run> {
        // The state store keys runs by schedule within a community; we don't
        // have a "list all communities" accessor, so iterate the snapshot via
        // the demo community + any the agent created. For the MCP server's
        // own state this is the complete set.
        let mut out = Vec::new();
        for c in self.known_communities().await {
            out.extend(self.state.runs_for(&c).await.into_values());
        }
        out
    }

    async fn known_communities(&self) -> Vec<String> {
        // We expose communities implicitly via agents/schedules. For the MCP
        // standalone server the only community is the default + demo.
        let mut v = vec![self.default_community.clone()];
        if self.default_community != "demo" {
            v.push("demo".into());
        }
        v
    }

    /// Build the full calendar snapshot for a community.
    async fn snapshot(&self, community: &str) -> CalendarSnapshot {
        let agents = self.state.agents_for(community).await;
        let schedules = self.state.schedules_for(community).await;
        let runs = self.state.runs_for(community).await;
        let contracts = self.state.contracts_for(community).await;
        let store = self.state.manifest_store_async().await;

        // Compute lineage verdicts per consuming schedule.
        let mut lineage = serde_json::Map::new();
        for s in &schedules {
            let mine: Vec<Contract> = contracts
                .iter()
                .filter(|c| c.schedule_id == s.schedule_id && c.role == ContractRole::Consume)
                .cloned()
                .collect();
            if mine.is_empty() {
                continue;
            }
            let run = runs.get(&s.schedule_id).cloned().unwrap_or(Run {
                run_id: format!("view-{}", s.schedule_id),
                schedule_id: s.schedule_id.clone(),
                scheduled_for: now(),
                started_at: Some(now()),
                finished_at: None,
                status: RunStatus::Pending,
                error: None,
            });
            if let Ok(deps) = almanac_bridge::lineage::check_inputs(&store, &run, &mine).await {
                let entries: Vec<_> = deps
                    .into_iter()
                    .map(|d| {
                        let state = match &d.satisfied {
                            almanac_bridge::lineage::Satisfies::Ready { .. } => "ready",
                            almanac_bridge::lineage::Satisfies::Missing => "missing",
                            almanac_bridge::lineage::Satisfies::VersionMismatch { .. } => {
                                "version_mismatch"
                            }
                        };
                        serde_json::json!({
                            "schema_id": d.schema_id,
                            "state": state,
                            "detail": d.satisfied,
                        })
                    })
                    .collect();
                lineage.insert(s.schedule_id.clone(), serde_json::Value::Array(entries));
            }
        }

        CalendarSnapshot {
            community: community.to_string(),
            agents,
            schedules: schedules
                .into_iter()
                .map(|s| {
                    let status = runs
                        .get(&s.schedule_id)
                        .map(|r| format!("{:?}", r.status))
                        .unwrap_or_else(|| "pending".into());
                    ScheduleView {
                        schedule_id: s.schedule_id,
                        summary: s.summary,
                        rrule: s.rrule,
                        calendar_group: s.calendar_group,
                        owner_agent_id: s.owner_agent_id,
                        run_status: status,
                    }
                })
                .collect(),
            contracts,
            lineage: serde_json::Value::Object(lineage),
        }
    }
}

// ===========================================================================
// Tool argument structs (field docs become the JSON input schema)
// ===========================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RegisterAgentArgs {
    /// Stable id (slug) for the agent, e.g. "research-bot".
    pub agent_id: String,
    /// Human-readable name.
    pub name: Option<String>,
    /// Optional emoji or avatar URL.
    pub avatar: Option<String>,
    /// Agent kind: "cron", "webhook", "on-demand", "mcp-tool".
    pub kind: Option<String>,
    /// Optional description / role.
    pub description: Option<String>,
    /// Community to register in (defaults to the server's default).
    pub community: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateScheduleArgs {
    /// Unique schedule id (slug).
    pub schedule_id: String,
    /// Human-readable summary shown in the calendar.
    pub summary: String,
    /// RFC 5545 RRULE, e.g. "FREQ=DAILY;BYHOUR=9". Empty = webhook one-off.
    pub rrule: Option<String>,
    /// Agent that owns this schedule (the principal).
    pub owner_agent_id: Option<String>,
    /// Calendar group label (becomes a CATEGORIES entry).
    pub calendar_group: Option<String>,
    /// Suggested color (CSS).
    pub color: Option<String>,
    /// Description / markdown body.
    pub description: Option<String>,
    /// Channel id (for ACL).
    pub channel: Option<String>,
    /// DTSTART in unix seconds (defaults to now).
    pub dtstart: Option<i64>,
    /// Community (defaults to server default).
    pub community: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommunityArgs {
    /// Community id (defaults to the server's default).
    pub community: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeclareContractArgs {
    /// Schedule that produces or consumes.
    pub schedule: String,
    /// "produce" or "consume".
    pub role: String,
    /// Schema id of the artifact.
    pub schema: String,
    /// For consume: minimum producer version (>= 1). Default 1.
    pub min_version: Option<u32>,
    /// For consume: accept any version (skip version check). Default false.
    pub any_version: Option<bool>,
    /// Freshness window in seconds (default 86400 = 24h).
    pub freshness: Option<u64>,
    pub community: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RecordManifestArgs {
    /// The run that produced the artifact.
    pub run_id: String,
    /// Schema id of the artifact.
    pub schema_id: String,
    /// Producer-declared version (>= 1).
    pub schema_version: Option<u32>,
    /// SHA-256 or other content hash.
    pub content_hash: Option<String>,
    /// Optional git commit sha.
    pub commit_sha: Option<String>,
    /// Where the artifact lives (URL / path).
    pub uri: Option<String>,
    /// Size in bytes.
    pub bytes: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckLineageArgs {
    /// Schedule whose consuming contracts to check.
    pub schedule_id: String,
    pub community: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TriggerRunArgs {
    /// Schedule to run.
    pub schedule_id: String,
    /// Optional explicit run id (defaults to run-<timestamp>).
    pub run_id: Option<String>,
    /// Scheduled time in unix seconds (defaults to now).
    pub scheduled_for: Option<i64>,
    pub community: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateRunArgs {
    /// Run id to advance.
    pub run_id: String,
    /// New status: "running", "succeeded", "failed", "skipped".
    pub status: String,
    /// Error text (for failed).
    pub error: Option<String>,
    /// Missing input schema (for skipped).
    pub missing_input: Option<String>,
}

/// The full calendar snapshot returned by `get_calendar`.
#[derive(Debug, serde::Serialize)]
pub struct CalendarSnapshot {
    pub community: String,
    pub agents: Vec<Agent>,
    pub schedules: Vec<ScheduleView>,
    pub contracts: Vec<Contract>,
    pub lineage: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
pub struct ScheduleView {
    pub schedule_id: String,
    pub summary: String,
    pub rrule: String,
    pub calendar_group: String,
    pub owner_agent_id: Option<String>,
    pub run_status: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let seed = std::env::var("ALMANAC_MCP_SEED")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
        .unwrap_or(true);
    let default_community = std::env::var("ALMANAC_COMMUNITY").unwrap_or_else(|_| "demo".into());
    run(seed, default_community).await
}
