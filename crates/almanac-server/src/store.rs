//! In-memory Almanac state store.
//!
//! Holds the schedules, runs, contracts, manifests, and calendars Almanac
//! has observed. The HTTP server reads from this to render feeds; the
//! ingestion endpoints write to it. In a Buzz deployment this would be
//! replaced by queries against the relay's event store — the shapes are
//! identical.

use almanac_bridge::model::{Calendar, Contract, Manifest, Run, Schedule};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// The full Almanac state, keyed by community.
#[derive(Debug, Default, Clone)]
pub struct State {
    inner: Arc<RwLock<RawState>>,
}

#[derive(Debug, Default)]
struct RawState {
    /// community_id -> schedules (keyed by schedule_id).
    schedules: HashMap<String, HashMap<String, Schedule>>,
    /// schedule_id -> latest run (one overlay per schedule in v1).
    runs: HashMap<String, Run>,
    /// community_id -> contracts.
    contracts: HashMap<String, Vec<Contract>>,
    /// community_id -> calendars.
    calendars: HashMap<String, Vec<Calendar>>,
    /// manifest store shared with the lineage engine.
    manifests: almanac_bridge::lineage::InMemoryManifestStore,
}

impl State {
    /// Create an empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a reference to the underlying manifest store (for lineage checks).
    pub fn manifest_store(&self) -> almanac_bridge::lineage::InMemoryManifestStore {
        let inner = self.inner.blocking_read();
        inner.manifests.clone()
    }

    /// Insert or update a schedule (LWW by `updated_at`).
    pub async fn upsert_schedule(&self, schedule: Schedule) {
        let mut w = self.inner.write().await;
        let bucket = w
            .schedules
            .entry(schedule.community_id.clone())
            .or_default();
        let id = schedule.schedule_id.clone();
        bucket.insert(id, schedule);
    }

    /// Insert or update a run (latest wins per schedule).
    pub async fn upsert_run(&self, run: Run) {
        let mut w = self.inner.write().await;
        w.runs.insert(run.schedule_id.clone(), run);
    }

    /// Add a contract.
    pub async fn add_contract(&self, community_id: &str, contract: Contract) {
        let mut w = self.inner.write().await;
        w.contracts
            .entry(community_id.to_string())
            .or_default()
            .push(contract);
    }

    /// Add a calendar.
    pub async fn add_calendar(&self, calendar: Calendar) {
        let mut w = self.inner.write().await;
        w.calendars
            .entry(calendar.community_id.clone())
            .or_default()
            .push(calendar);
    }

    /// Insert a manifest (delegates to the manifest store).
    pub async fn put_manifest(&self, manifest: Manifest) {
        let manifests = {
            let r = self.inner.read().await;
            r.manifests.clone()
        };
        manifests.put(manifest).await;
    }

    /// Snapshot all schedules in a community (in stable order).
    pub async fn schedules_for(&self, community: &str) -> Vec<Schedule> {
        let r = self.inner.read().await;
        let Some(bucket) = r.schedules.get(community) else {
            return Vec::new();
        };
        let mut out: Vec<Schedule> = bucket.values().cloned().collect();
        out.sort_by(|a, b| a.schedule_id.cmp(&b.schedule_id));
        out
    }

    /// Snapshot the latest run per schedule (community-scoped via schedules).
    pub async fn runs_for(&self, community: &str) -> HashMap<String, Run> {
        let r = self.inner.read().await;
        let Some(bucket) = r.schedules.get(community) else {
            return HashMap::new();
        };
        let mut out = HashMap::new();
        for sid in bucket.keys() {
            if let Some(run) = r.runs.get(sid) {
                out.insert(sid.clone(), run.clone());
            }
        }
        out
    }

    /// Snapshot contracts for a community.
    pub async fn contracts_for(&self, community: &str) -> Vec<Contract> {
        let r = self.inner.read().await;
        r.contracts.get(community).cloned().unwrap_or_default()
    }

    /// Snapshot calendars for a community.
    pub async fn calendars_for(&self, community: &str) -> Vec<Calendar> {
        let r = self.inner.read().await;
        r.calendars.get(community).cloned().unwrap_or_default()
    }

    /// Clone the manifest store handle for async lineage checks.
    pub async fn manifest_store_async(&self) -> almanac_bridge::lineage::InMemoryManifestStore {
        self.inner.read().await.manifests.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almanac_bridge::model::{RunStatus, Schedule};

    fn sched(id: &str, community: &str) -> Schedule {
        Schedule {
            schedule_id: id.into(),
            community_id: community.into(),
            channel_id: "c".into(),
            summary: id.into(),
            description: "d".into(),
            rrule: "FREQ=DAILY".into(),
            dtstart: 1,
            calendar_group: "g".into(),
            color_category: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[tokio::test]
    async fn upsert_and_read_schedules() {
        let s = State::new();
        s.upsert_schedule(sched("a", "research")).await;
        s.upsert_schedule(sched("b", "research")).await;
        s.upsert_schedule(sched("c", "other")).await;
        let got = s.schedules_for("research").await;
        assert_eq!(got.len(), 2);
        assert!(got.iter().any(|x| x.schedule_id == "a"));
        assert!(got.iter().any(|x| x.schedule_id == "b"));
        assert_eq!(s.schedules_for("other").await.len(), 1);
        assert!(s.schedules_for("missing").await.is_empty());
    }

    #[tokio::test]
    async fn upsert_run_overwrites() {
        let s = State::new();
        s.upsert_schedule(sched("a", "research")).await;
        let mut r1 = Run {
            run_id: "r1".into(),
            schedule_id: "a".into(),
            scheduled_for: 1,
            started_at: None,
            finished_at: None,
            status: RunStatus::Pending,
            error: None,
        };
        s.upsert_run(r1.clone()).await;
        r1.status = RunStatus::Succeeded;
        s.upsert_run(r1).await;
        let runs = s.runs_for("research").await;
        assert_eq!(runs.len(), 1);
        assert_eq!(runs.get("a").unwrap().status, RunStatus::Succeeded);
    }
}
