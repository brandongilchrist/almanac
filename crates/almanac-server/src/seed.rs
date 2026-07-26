//! Demo dataset seeder.
//!
//! Seeds a believable two-schedule lineage scenario so the calendar feed is
//! non-empty out of the box:
//!
//! - `daily-brief` produces `research-brief` every day at 09:00.
//! - `weekly-strategy` consumes `research-brief` every Monday at 10:00.
//!
//! The daily brief's latest run is `Succeeded` with a materialized manifest,
//! so the weekly strategy's lineage check shows ✅.

use crate::store::State;
use almanac_bridge::lineage::InMemoryManifestStore;
use almanac_bridge::model::{
    Contract, ContractRole, Manifest, Run, RunStatus, Schedule, SkipReason,
};

const NOW: i64 = 1_700_000_000;

fn daily_brief() -> Schedule {
    Schedule {
        schedule_id: "daily-brief".into(),
        community_id: "demo".into(),
        channel_id: "demo-channel".into(),
        summary: "Daily research brief".into(),
        description: "Produces a research brief every morning at 09:00 UTC.".into(),
        rrule: "FREQ=DAILY;BYHOUR=9;BYMINUTE=0".into(),
        dtstart: NOW,
        calendar_group: "research".into(),
        color_category: Some("#3b82f6".into()),
        created_at: NOW,
        updated_at: NOW,
    }
}

fn weekly_strategy() -> Schedule {
    Schedule {
        schedule_id: "weekly-strategy".into(),
        community_id: "demo".into(),
        channel_id: "demo-channel".into(),
        summary: "Weekly strategy draft".into(),
        description: "Drafts strategy from the week's research briefs. Mondays at 10:00 UTC."
            .into(),
        rrule: "FREQ=WEEKLY;BYDAY=MO;BYHOUR=10;BYMINUTE=0".into(),
        dtstart: NOW,
        calendar_group: "strategy".into(),
        color_category: Some("#10b981".into()),
        created_at: NOW,
        updated_at: NOW,
    }
}

fn nightly_index() -> Schedule {
    Schedule {
        schedule_id: "nightly-index".into(),
        community_id: "demo".into(),
        channel_id: "demo-channel".into(),
        summary: "Nightly vector index rebuild".into(),
        description: "Rebuilds the search index. Currently failing.".into(),
        rrule: "FREQ=DAILY;BYHOUR=2;BYMINUTE=0".into(),
        dtstart: NOW,
        calendar_group: "infra".into(),
        color_category: Some("#ef4444".into()),
        created_at: NOW,
        updated_at: NOW,
    }
}

fn webhook_pr_review() -> Schedule {
    Schedule {
        schedule_id: "pr-review".into(),
        community_id: "demo".into(),
        channel_id: "demo-channel".into(),
        summary: "On PR merge: review summary".into(),
        description: "Webhook-triggered. Produces a review summary on every PR merge.".into(),
        rrule: String::new(), // webhook — one-off
        dtstart: NOW,
        calendar_group: "code".into(),
        color_category: Some("#8b5cf6".into()),
        created_at: NOW,
        updated_at: NOW,
    }
}

fn daily_brief_run() -> Run {
    Run {
        run_id: "run-daily-001".into(),
        schedule_id: "daily-brief".into(),
        scheduled_for: NOW + 3_600,
        started_at: Some(NOW + 3_610),
        finished_at: Some(NOW + 3_900),
        status: RunStatus::Succeeded,
        error: None,
    }
}

fn nightly_index_run() -> Run {
    Run {
        run_id: "run-index-001".into(),
        schedule_id: "nightly-index".into(),
        scheduled_for: NOW + 7_200,
        started_at: Some(NOW + 7_210),
        finished_at: Some(NOW + 7_280),
        status: RunStatus::Failed,
        error: Some("index writer OOM (exit 137)".into()),
    }
}

fn weekly_strategy_run_pending() -> Run {
    Run {
        run_id: "run-weekly-001".into(),
        schedule_id: "weekly-strategy".into(),
        scheduled_for: NOW + 86_400,
        started_at: None,
        finished_at: None,
        status: RunStatus::Pending,
        error: None,
    }
}

fn webhook_pr_review_run() -> Run {
    Run {
        run_id: "run-pr-042".into(),
        schedule_id: "pr-review".into(),
        scheduled_for: NOW + 1_800,
        started_at: Some(NOW + 1_810),
        finished_at: Some(NOW + 2_400),
        status: RunStatus::Succeeded,
        error: None,
    }
}

fn research_brief_manifest() -> Manifest {
    Manifest {
        manifest_id: Manifest::id_for("run-daily-001", "research-brief"),
        run_id: "run-daily-001".into(),
        schema_id: "research-brief".into(),
        schema_version: 3,
        content_hash: "a1b2c3d4e5f6".into(),
        commit_sha: Some("deadbeefcafebabe1234567890abcdef12345678".into()),
        uri: "https://github.com/demo/research/commit/deadbeefcafebabe1234567890abcdef12345678"
            .into(),
        bytes: Some(48_213),
        materialized_at: NOW + 3_900,
    }
}

fn contracts() -> Vec<Contract> {
    vec![
        Contract {
            contract_id: "daily-brief-produces".into(),
            schedule_id: "daily-brief".into(),
            role: ContractRole::Produce,
            schema_id: "research-brief".into(),
            min_version: 1,
            any_version: false,
            freshness_window: Contract::DEFAULT_FRESHNESS_WINDOW,
        },
        Contract {
            contract_id: "weekly-strategy-consumes".into(),
            schedule_id: "weekly-strategy".into(),
            role: ContractRole::Consume,
            schema_id: "research-brief".into(),
            min_version: 2,
            any_version: false,
            freshness_window: 604_800, // 7 days — weekly consumer
        },
    ]
}

/// Seed the demo community into `state`.
pub async fn seed_demo(state: &State) {
    for sched in [
        daily_brief(),
        weekly_strategy(),
        nightly_index(),
        webhook_pr_review(),
    ] {
        state.upsert_schedule(sched).await;
    }
    for run in [
        daily_brief_run(),
        nightly_index_run(),
        weekly_strategy_run_pending(),
        webhook_pr_review_run(),
    ] {
        state.upsert_run(run).await;
    }
    state.put_manifest(research_brief_manifest()).await;
    for c in contracts() {
        state.add_contract("demo", c).await;
    }
}

/// Standalone helper: return a freshly-seeded manifest store (used by tests
/// and the demo CLI).
pub async fn seeded_manifest_store() -> InMemoryManifestStore {
    let store = InMemoryManifestStore::new();
    store.put(research_brief_manifest()).await;
    store
}

/// Skip-reason demo data, for documenting the ✗ state.
pub fn example_skip_reason() -> SkipReason {
    SkipReason::MissingInput("research-brief".into())
}
