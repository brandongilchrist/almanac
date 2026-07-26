//! The [`ManifestStore`] trait and an in-memory test implementation.
//!
//! This is the only place Almanac *reads* lineage state. The production impl
//! queries Buzz's event store for `KIND_ALMANAC_MANIFEST` events; the test
//! impl is in-memory. Almanac never writes lineage state except via events.

use crate::error::LineageError;
use crate::model::Manifest;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Read-only access to the set of materialized manifests.
///
/// Implementations must be cheap to call — this runs on every consumer fire.
#[async_trait]
pub trait ManifestStore: Send + Sync {
    /// Return the most recent manifest matching `schema_id`, or `None`.
    /// The caller applies the freshness window.
    async fn latest_for_schema(&self, schema_id: &str) -> Result<Option<Manifest>, LineageError>;

    /// Return all manifests matching `schema_id` (newest first), up to `limit`.
    async fn history_for_schema(
        &self,
        schema_id: &str,
        limit: usize,
    ) -> Result<Vec<Manifest>, LineageError>;
}

/// An in-memory [`ManifestStore`] for tests and the demo server.
#[derive(Debug, Default, Clone)]
pub struct InMemoryManifestStore {
    inner: Arc<RwLock<HashMap<String, Vec<Manifest>>>>,
}

impl InMemoryManifestStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a manifest (or update an existing one with the same id, LWW).
    pub async fn put(&self, manifest: Manifest) {
        let mut w = self.inner.write().await;
        let bucket = w.entry(manifest.schema_id.clone()).or_default();
        // Replace if same manifest_id exists, else push.
        if let Some(slot) = bucket
            .iter_mut()
            .find(|m| m.manifest_id == manifest.manifest_id)
        {
            *slot = manifest;
        } else {
            bucket.push(manifest);
        }
        // Keep newest-first.
        bucket.sort_by(|a, b| b.materialized_at.cmp(&a.materialized_at));
    }

    /// Bulk-insert manifests.
    pub async fn extend<I: IntoIterator<Item = Manifest>>(&self, manifests: I) {
        for m in manifests {
            self.put(m).await;
        }
    }
}

#[async_trait]
impl ManifestStore for InMemoryManifestStore {
    async fn latest_for_schema(&self, schema_id: &str) -> Result<Option<Manifest>, LineageError> {
        let r = self.inner.read().await;
        Ok(r.get(schema_id).and_then(|bucket| bucket.first().cloned()))
    }

    async fn history_for_schema(
        &self,
        schema_id: &str,
        limit: usize,
    ) -> Result<Vec<Manifest>, LineageError> {
        let r = self.inner.read().await;
        Ok(r.get(schema_id)
            .map(|bucket| bucket.iter().take(limit).cloned().collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(schema_id: &str, version: u32, at: i64) -> Manifest {
        Manifest {
            manifest_id: format!("run-{at}:{schema_id}"),
            run_id: format!("run-{at}"),
            schema_id: schema_id.into(),
            schema_version: version,
            content_hash: format!("hash-{at}"),
            commit_sha: None,
            uri: format!("uri-{at}"),
            bytes: None,
            materialized_at: at,
        }
    }

    #[tokio::test]
    async fn latest_returns_newest() {
        let s = InMemoryManifestStore::new();
        s.put(mk("brief", 1, 100)).await;
        s.put(mk("brief", 2, 50)).await;
        s.put(mk("brief", 3, 200)).await;
        let latest = s.latest_for_schema("brief").await.unwrap().unwrap();
        assert_eq!(latest.materialized_at, 200);
    }

    #[tokio::test]
    async fn put_lww_replaces_same_id() {
        let s = InMemoryManifestStore::new();
        let mut m = mk("brief", 1, 100);
        s.put(m.clone()).await;
        // Same manifest_id, new version.
        m.schema_version = 5;
        s.put(m.clone()).await;
        let all = s.history_for_schema("brief", 10).await.unwrap();
        assert_eq!(all.len(), 1, "LWW replaced, not appended");
        assert_eq!(all[0].schema_version, 5);
    }

    #[tokio::test]
    async fn missing_schema_returns_none() {
        let s = InMemoryManifestStore::new();
        assert!(s.latest_for_schema("nope").await.unwrap().is_none());
        assert!(s.history_for_schema("nope", 10).await.unwrap().is_empty());
    }
}
