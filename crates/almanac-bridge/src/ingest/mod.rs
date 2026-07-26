//! Ingestion: translate source Nostr events into Almanac structs.
//!
//! - [`schedule`] — `KIND_WORKFLOW_DEF` (30620) → [`Schedule`].
//! - [`run`] — fire-claim + agent output → [`Run`] with status.
//! - [`manifest`] — agent output → [`Manifest`] (best-effort).

pub mod manifest;
pub mod run;
pub mod schedule;

pub use manifest::manifest_from_agent_output;
pub use run::{run_from_fire_claim, transition};
pub use schedule::schedule_from_workflow_def;
