//! Core Almanac data model.
//!
//! Six entities, faithful to `docs/10_PLAN.md` § "Event kinds" and
//! § "Resolved data-model decisions":
//!
//! - [`Agent`] — an agent identity (the principal owning schedules/runs).
//! - [`Schedule`] — the recurring cron definition (the plan).
//! - [`Run`] — one concrete execution of a schedule (produces a manifest).
//! - [`Manifest`] — an artifact's materialization record (the lineage primitive).
//! - [`Contract`] — a producer/consumer dependency declaration.
//! - [`Calendar`] — calendar grouping/metadata.
//!
//! All types are `Serialize`/`Deserialize` and round-trip through `serde_json`.

use serde::{Deserialize, Serialize};

/// An agent identity — the principal that owns schedules and produces manifests.
///
/// Agents are first-class citizens in Almanac (analogous to users in a comms
/// app): every schedule and run is attributed to one. Agents are registered
/// once and referenced by `agent_id` thereafter, so the calendar UI can group
/// work by the agent that does it and show an "agents panel".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    /// Stable id (slug). Becomes the schedule's `owner_agent_id`.
    pub agent_id: String,
    /// Display name shown in the UI.
    pub name: String,
    /// Optional avatar URL / emoji.
    pub avatar: Option<String>,
    /// Free-form kind: `cron`, `webhook`, `on-demand`, `mcp-tool`, …
    pub kind: String,
    /// Owning community.
    pub community_id: String,
    /// Optional description / role.
    pub description: Option<String>,
    /// Registration time (unix seconds).
    pub created_at: i64,
}

/// A cron definition as seen by Almanac — a mirror of a workflow def with
/// calendar-render hints (color, summary template, calendar subgroup).
///
/// `schedule_id` is the NIP-33 `d` tag and the VEVENT `UID` suffix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    /// NIP-33 `d` tag; unique within `(pubkey, kind)`. Becomes `<id>@almanac` UID.
    pub schedule_id: String,
    /// Owning community; scopes the calendar feed.
    pub community_id: String,
    /// Nostr channel the schedule writes to (for ACL; may be empty).
    pub channel_id: String,
    /// Human-readable name shown in `SUMMARY` (before emoji overlay).
    pub summary: String,
    /// Markdown body shown in `DESCRIPTION`.
    pub description: String,
    /// RRULE string (RFC 5545), e.g. `FREQ=DAILY;BYHOUR=9`.
    /// Empty for webhook-triggered one-off schedules.
    pub rrule: String,
    /// Starting DTSTART in ICS (RFC 5545) `UTC` format seconds.
    pub dtstart: i64,
    /// Calendar subgroup label (becomes a `CATEGORIES` entry: `almanac:<group>`).
    pub calendar_group: String,
    /// Suggested color for clients that map categories → colors.
    pub color_category: Option<String>,
    /// Agent that owns this schedule (agent-native attribution). Optional for back-compat.
    pub owner_agent_id: Option<String>,
    /// Creation time (unix seconds).
    pub created_at: i64,
    /// Last update time (unix seconds).
    pub updated_at: i64,
}

/// One concrete execution of a schedule. Concrete; produces a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// NIP-33 `d` tag; unique within `(pubkey, kind)`.
    pub run_id: String,
    /// Parent schedule id.
    pub schedule_id: String,
    /// When the run was scheduled to fire (unix seconds).
    pub scheduled_for: i64,
    /// When the run actually started, if it has (unix seconds).
    pub started_at: Option<i64>,
    /// When the run finished, if it has (unix seconds).
    pub finished_at: Option<i64>,
    /// Lifecycle status.
    pub status: RunStatus,
    /// Error / skip reason, if any.
    pub error: Option<String>,
}

/// Run lifecycle. Maps to iCalendar `STATUS` and an emoji `SUMMARY` prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail")]
pub enum RunStatus {
    /// Scheduled, hasn't run yet this cycle.
    Pending,
    /// Currently executing.
    Running,
    /// Ran, produced a manifest, verified.
    Succeeded,
    /// Ran, exited non-zero / agent error.
    Failed,
    /// Inputs missing or version-mismatched; refused to start.
    Skipped(SkipReason),
}

/// Why a run was skipped instead of executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkipReason {
    /// No manifest for the named schema within the freshness window.
    MissingInput(String),
    /// A manifest exists but its `schema_version` < the consumer's `min_version`.
    VersionMismatch(String),
}

/// An artifact's materialization record. **The lineage primitive.**
///
/// One manifest per `(run, schema_id)`. The NIP-33 `d` tag is `<run_id>:<schema_id>`
/// (see [`Manifest::id_for`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// `<run_id>:<schema_id>` — see [`Manifest::id_for`].
    pub manifest_id: String,
    /// Producing run id.
    pub run_id: String,
    /// Logical artifact type (e.g. `research-brief`, `agent-output`).
    pub schema_id: String,
    /// Monotonic producer-declared version (≥ 1).
    pub schema_version: u32,
    /// SHA-256 hex of the artifact content (or agent output content as fallback).
    pub content_hash: String,
    /// Optional git commit sha if the artifact landed in a repo.
    pub commit_sha: Option<String>,
    /// Where the artifact lives (GitHub URL / Buzz thread / file path).
    pub uri: String,
    /// Size in bytes, if known.
    pub bytes: Option<u64>,
    /// When the artifact was materialized (unix seconds).
    pub materialized_at: i64,
}

impl Manifest {
    /// Build the canonical manifest id for a `(run, schema)` pair.
    ///
    /// This is the NIP-33 `d` tag and the LWW key: re-emitting the same
    /// artifact for the same run updates it in place rather than duplicating.
    pub fn id_for(run_id: &str, schema_id: &str) -> String {
        format!("{run_id}:{schema_id}")
    }
}

/// A producer/consumer contract: declares what a schedule *expects to
/// produce* or *expects to consume* (schema id + version).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    /// NIP-33 `d` tag.
    pub contract_id: String,
    /// Owning schedule.
    pub schedule_id: String,
    /// Produce vs consume.
    pub role: ContractRole,
    /// Logical artifact type this contract concerns.
    pub schema_id: String,
    /// For `Consume`: minimum acceptable producer version (≥ 1).
    /// Ignored for `Produce`.
    pub min_version: u32,
    /// For `Consume`: if `true`, skip the version check entirely.
    /// Do **not** overload `min_version: 0` as a sentinel — versions start at 1.
    pub any_version: bool,
    /// Freshness window in seconds (default 86400 = 24h).
    pub freshness_window: u64,
}

impl Contract {
    /// The default freshness window (24 hours), per `10_PLAN.md` decision #3.
    pub const DEFAULT_FRESHNESS_WINDOW: u64 = 86_400;
}

/// Which side of a contract a schedule is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractRole {
    /// The schedule emits manifests for this schema.
    Produce,
    /// The schedule consumes manifests for this schema.
    Consume,
}

/// Calendar grouping/metadata. Lets a community publish multiple calendars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    /// NIP-33 `d` tag.
    pub calendar_id: String,
    /// Owning community.
    pub community_id: String,
    /// Display name (becomes `X-WR-CALNAME`).
    pub name: String,
    /// Human description.
    pub description: String,
    /// Suggested color (becomes `X-APPLE-CALENDAR-COLOR` hint).
    pub color: Option<String>,
    /// Schedules that belong to this calendar.
    pub schedule_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_schedule() -> Schedule {
        Schedule {
            schedule_id: "daily-brief".into(),
            community_id: "research".into(),
            channel_id: "chan-1".into(),
            summary: "Daily research brief".into(),
            description: "Produces a research brief every morning.".into(),
            rrule: "FREQ=DAILY;BYHOUR=9".into(),
            dtstart: 1_700_000_000,
            calendar_group: "research".into(),
            color_category: Some("#3b82f6".into()),
            owner_agent_id: None,
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
        }
    }

    fn sample_run() -> Run {
        Run {
            run_id: "run-1".into(),
            schedule_id: "daily-brief".into(),
            scheduled_for: 1_700_003_600,
            started_at: Some(1_700_003_610),
            finished_at: Some(1_700_003_900),
            status: RunStatus::Succeeded,
            error: None,
        }
    }

    fn sample_manifest() -> Manifest {
        Manifest {
            manifest_id: Manifest::id_for("run-1", "research-brief"),
            run_id: "run-1".into(),
            schema_id: "research-brief".into(),
            schema_version: 3,
            content_hash: "abc123".into(),
            commit_sha: Some("deadbeef".into()),
            uri: "https://github.com/org/repo/commit/deadbeef".into(),
            bytes: Some(2048),
            materialized_at: 1_700_003_900,
        }
    }

    fn sample_contract() -> Contract {
        Contract {
            contract_id: "c1".into(),
            schedule_id: "weekly-strategy".into(),
            role: ContractRole::Consume,
            schema_id: "research-brief".into(),
            min_version: 2,
            any_version: false,
            freshness_window: Contract::DEFAULT_FRESHNESS_WINDOW,
        }
    }

    fn sample_calendar() -> Calendar {
        Calendar {
            calendar_id: "cal-1".into(),
            community_id: "research".into(),
            name: "Research Schedules".into(),
            description: "All research schedules.".into(),
            color: Some("#3b82f6".into()),
            schedule_ids: vec!["daily-brief".into()],
        }
    }

    #[test]
    fn schedule_round_trips() {
        let s = sample_schedule();
        let json = serde_json::to_string(&s).unwrap();
        let back: Schedule = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn run_round_trips_all_status_variants() {
        let base = sample_run();
        let variants = [
            RunStatus::Pending,
            RunStatus::Running,
            RunStatus::Succeeded,
            RunStatus::Failed,
            RunStatus::Skipped(SkipReason::MissingInput("research-brief".into())),
            RunStatus::Skipped(SkipReason::VersionMismatch("research-brief".into())),
        ];
        for st in variants {
            let mut r = base.clone();
            r.status = st.clone();
            let json = serde_json::to_string(&r).unwrap();
            let back: Run = serde_json::from_str(&json).unwrap();
            assert_eq!(r, back, "round-trip failed for {st:?}");
        }
    }

    #[test]
    fn manifest_round_trips() {
        let m = sample_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn contract_round_trips_both_roles() {
        for role in [ContractRole::Produce, ContractRole::Consume] {
            let mut c = sample_contract();
            c.role = role;
            let json = serde_json::to_string(&c).unwrap();
            let back: Contract = serde_json::from_str(&json).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn calendar_round_trips() {
        let c = sample_calendar();
        let json = serde_json::to_string(&c).unwrap();
        let back: Calendar = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn manifest_id_for_format() {
        assert_eq!(
            Manifest::id_for("run-1", "research-brief"),
            "run-1:research-brief"
        );
        assert_eq!(
            Manifest::id_for("run-1", "research-brief"),
            sample_manifest().manifest_id
        );
    }

    #[test]
    fn default_freshness_is_24h() {
        assert_eq!(Contract::DEFAULT_FRESHNESS_WINDOW, 86_400);
    }
}
