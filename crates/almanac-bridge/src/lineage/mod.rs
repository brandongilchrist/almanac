//! Lineage engine.
//!
//! - [`check`] — the "is my input materialized?" query.
//! - [`graph`] — derive consumer→producer edges from contracts (optional, Phase 3).
//! - [`store`] — the [`ManifestStore`] trait + an in-memory test impl.

pub mod check;
pub mod graph;
pub mod store;

pub use check::{check_inputs, Dependency, Satisfies};
pub use graph::{derive_edges, Edge};
pub use store::{InMemoryManifestStore, ManifestStore};
