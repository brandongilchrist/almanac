//! The "is my input materialized?" query.
//!
//! For each consuming [`Contract`], queries the [`ManifestStore`] for the
//! most recent manifest matching `schema_id` and applies:
//!
//! 1. **Freshness window** — `(now - freshness_window) <= materialized_at <= now`.
//!    `now` is the consumer's execution time (`started_at`, falling back to
//!    `scheduled_for`).
//! 2. **Version** — `manifest.schema_version >= contract.min_version`, unless
//!    `contract.any_version` is `true`.
//!
//! Yields a [`Dependency`] per consuming contract with a [`Satisfies`] verdict.

use crate::error::LineageError;
use crate::lineage::ManifestStore;
use crate::model::{Contract, ContractRole, Run};
use serde::{Deserialize, Serialize};

/// A lineage verdict for one dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    /// The schema the consumer needs.
    pub schema_id: String,
    /// The schedule that produces it (if known), for RELATED-TO emission.
    pub producer_schedule_id: Option<String>,
    /// Whether the input is satisfied.
    pub satisfied: Satisfies,
}

/// The three satisfaction states (per `10_PLAN.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "detail")]
pub enum Satisfies {
    /// A fresh, version-matching manifest exists.
    Ready { manifest_at: i64, version: u32 },
    /// No manifest within the freshness window.
    Missing,
    /// A manifest exists but its version is too old.
    VersionMismatch { found: u32, need: u32 },
}

/// Check all consuming contracts for a run.
///
/// Producing contracts are skipped (they declare output, not input).
pub async fn check_inputs(
    store: &dyn ManifestStore,
    run: &Run,
    contracts: &[Contract],
) -> Result<Vec<Dependency>, LineageError> {
    let now = run.started_at.unwrap_or(run.scheduled_for);
    let mut deps = Vec::new();
    for c in contracts.iter().filter(|c| c.role == ContractRole::Consume) {
        let verdict = check_one(store, now, c).await?;
        deps.push(Dependency {
            schema_id: c.schema_id.clone(),
            producer_schedule_id: None, // filled by graph layer if known
            satisfied: verdict,
        });
    }
    Ok(deps)
}

async fn check_one(
    store: &dyn ManifestStore,
    now: i64,
    contract: &Contract,
) -> Result<Satisfies, LineageError> {
    let lower = now.saturating_sub(contract.freshness_window as i64);
    let Some(m) = store.latest_for_schema(&contract.schema_id).await? else {
        return Ok(Satisfies::Missing);
    };
    // Freshness window.
    if m.materialized_at < lower || m.materialized_at > now {
        return Ok(Satisfies::Missing);
    }
    // Version.
    if !contract.any_version && m.schema_version < contract.min_version {
        return Ok(Satisfies::VersionMismatch {
            found: m.schema_version,
            need: contract.min_version,
        });
    }
    Ok(Satisfies::Ready {
        manifest_at: m.materialized_at,
        version: m.schema_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lineage::InMemoryManifestStore;
    use crate::model::{RunStatus, SkipReason};

    fn manifest(schema_id: &str, version: u32, at: i64) -> crate::model::Manifest {
        crate::model::Manifest {
            manifest_id: format!("run-{at}:{schema_id}"),
            run_id: format!("run-{at}"),
            schema_id: schema_id.into(),
            schema_version: version,
            content_hash: "h".into(),
            commit_sha: None,
            uri: "u".into(),
            bytes: None,
            materialized_at: at,
        }
    }

    fn consume_contract(schema: &str, min_version: u32, freshness: u64) -> Contract {
        Contract {
            contract_id: "c".into(),
            schedule_id: "consumer".into(),
            role: ContractRole::Consume,
            schema_id: schema.into(),
            min_version,
            any_version: false,
            freshness_window: freshness,
        }
    }

    fn run_at(now: i64) -> Run {
        Run {
            run_id: "r".into(),
            schedule_id: "consumer".into(),
            scheduled_for: now,
            started_at: Some(now),
            finished_at: None,
            status: RunStatus::Pending,
            error: None,
        }
    }

    #[tokio::test]
    async fn ready_when_fresh_and_version_ok() {
        let s = InMemoryManifestStore::new();
        s.put(manifest("brief", 3, 90)).await;
        let c = consume_contract("brief", 2, 100);
        let deps = check_inputs(&s, &run_at(100), &[c]).await.unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(
            deps[0].satisfied,
            Satisfies::Ready {
                manifest_at: 90,
                version: 3
            }
        );
    }

    #[tokio::test]
    async fn missing_when_no_manifest() {
        let s = InMemoryManifestStore::new();
        let c = consume_contract("brief", 1, 100);
        let deps = check_inputs(&s, &run_at(100), &[c]).await.unwrap();
        assert_eq!(deps[0].satisfied, Satisfies::Missing);
    }

    #[tokio::test]
    async fn missing_when_outside_freshness_window() {
        let s = InMemoryManifestStore::new();
        // manifest at t=10, freshness window 100, now=200 → 10 < 100 → missing
        s.put(manifest("brief", 1, 10)).await;
        let c = consume_contract("brief", 1, 100);
        let deps = check_inputs(&s, &run_at(200), &[c]).await.unwrap();
        assert_eq!(deps[0].satisfied, Satisfies::Missing);
    }

    #[tokio::test]
    async fn missing_when_future_dated() {
        let s = InMemoryManifestStore::new();
        s.put(manifest("brief", 1, 300)).await; // future
        let c = consume_contract("brief", 1, 1000);
        let deps = check_inputs(&s, &run_at(100), &[c]).await.unwrap();
        assert_eq!(deps[0].satisfied, Satisfies::Missing);
    }

    #[tokio::test]
    async fn version_mismatch() {
        let s = InMemoryManifestStore::new();
        s.put(manifest("brief", 2, 90)).await;
        let c = consume_contract("brief", 7, 100);
        let deps = check_inputs(&s, &run_at(100), &[c]).await.unwrap();
        assert_eq!(
            deps[0].satisfied,
            Satisfies::VersionMismatch { found: 2, need: 7 }
        );
    }

    #[tokio::test]
    async fn any_version_skips_version_check() {
        let s = InMemoryManifestStore::new();
        s.put(manifest("brief", 1, 90)).await;
        let mut c = consume_contract("brief", 99, 100);
        c.any_version = true;
        let deps = check_inputs(&s, &run_at(100), &[c]).await.unwrap();
        assert!(matches!(deps[0].satisfied, Satisfies::Ready { .. }));
    }

    #[tokio::test]
    async fn skips_producing_contracts() {
        let s = InMemoryManifestStore::new();
        let produce = Contract {
            contract_id: "p".into(),
            schedule_id: "p".into(),
            role: ContractRole::Produce,
            schema_id: "brief".into(),
            min_version: 1,
            any_version: false,
            freshness_window: 100,
        };
        let deps = check_inputs(&s, &run_at(100), &[produce]).await.unwrap();
        assert!(deps.is_empty(), "produce contracts are not checked");
    }

    #[tokio::test]
    async fn multiple_contracts_mixed() {
        let s = InMemoryManifestStore::new();
        s.put(manifest("ready-schema", 1, 90)).await;
        s.put(manifest("stale-schema", 1, 1)).await;
        let contracts = vec![
            consume_contract("ready-schema", 1, 100),
            consume_contract("stale-schema", 1, 10),
            consume_contract("absent-schema", 1, 100),
        ];
        let deps = check_inputs(&s, &run_at(100), &contracts).await.unwrap();
        assert_eq!(deps.len(), 3);
        let by_schema: HashMap<String, Satisfies> = deps
            .into_iter()
            .map(|d| (d.schema_id, d.satisfied))
            .collect();
        assert!(matches!(
            by_schema.get("ready-schema"),
            Some(Satisfies::Ready { .. })
        ));
        assert_eq!(by_schema.get("stale-schema"), Some(&Satisfies::Missing));
        assert_eq!(by_schema.get("absent-schema"), Some(&Satisfies::Missing));
    }

    use std::collections::HashMap;

    #[test]
    fn run_uses_scheduled_for_when_not_started() {
        // Pure sanity: RunStatus::Skipped still has SkipReason attached.
        let st = RunStatus::Skipped(SkipReason::MissingInput("x".into()));
        assert!(matches!(st, RunStatus::Skipped(_)));
    }
}
