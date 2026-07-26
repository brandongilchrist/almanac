//! # almanac-bridge
//!
//! A calendar for agents and their artifacts. Almanac turns scheduled agent
//! work into standard iCalendar feeds (`RFC 5545` / `RFC 7986` / `RFC 9253`),
//! with artifact-lineage checkmarks: each scheduled job declares its inputs
//! and outputs, and the calendar shows ✅ when an input is materialized, ❌
//! when it's missing, and ⚠️ when its version is too old.
//!
//! This crate is the rendering + lineage core. It is Buzz-native in data
//! model (kinds 48050–48054, Nostr events) but dependency-free at the
//! boundary: the [`source::SourceEvent`] type is a minimal read-only view
//! of a Nostr event, so the core is fully testable without a Nostr SDK.
//!
//! ## Layers
//!
//! - [`model`] — `Schedule`, `Run`, `Manifest`, `Contract`, `Calendar`.
//! - [`ingest`] — translate source events into structs.
//! - [`ical`] — render structs into RFC-5545 VEVENTs + feeds.
//! - [`lineage`] — the "is my input materialized?" query.
//! - [`config`] — runtime configuration.

#![forbid(unsafe_code)]
#![warn(
    clippy::dbg_macro,
    clippy::print_stdout,
    rustdoc::broken_intra_doc_links
)]

pub mod config;
pub mod error;
pub mod ical;
pub mod ingest;
pub mod kinds;
pub mod lineage;
pub mod model;
pub mod source;

pub use config::Config;
pub use error::{IcalError, IngestError, LineageError};
pub use kinds::*;
pub use model::{
    Agent, Calendar, Contract, ContractRole, Manifest, Run, RunStatus, Schedule, SkipReason,
};
