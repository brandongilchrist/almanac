//! Almanac error types.

use thiserror::Error;

/// Errors raised by the ingestion layer (event → struct translation).
#[derive(Debug, Error)]
pub enum IngestError {
    /// A required tag was missing from the source event.
    #[error("missing required tag `{tag}` on event")]
    MissingTag { tag: &'static str },
    /// A tag was present but could not be parsed into the expected type.
    #[error("invalid `{tag}` tag value: {reason}")]
    InvalidTag { tag: &'static str, reason: String },
    /// The event kind is not one the ingestor handles.
    #[error("unexpected event kind {kind}")]
    UnexpectedKind { kind: u32 },
    /// A run state transition was illegal (e.g. Succeeded → Running).
    #[error("illegal run state transition: {from:?} -> {to:?}")]
    IllegalTransition { from: String, to: String },
    /// Generic wrapping for serde failures during ingestion.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Errors raised by the ICS rendering layer.
#[derive(Debug, Error)]
pub enum IcalError {
    /// A schedule's RRULE could not be parsed.
    #[error("invalid RRULE `{rrule}`: {reason}")]
    InvalidRrule { rrule: String, reason: String },
    /// The underlying `icalendar` builder rejected a value.
    #[error("icalendar build error: {0}")]
    Build(String),
}

/// Errors raised by the lineage engine.
#[derive(Debug, Error)]
pub enum LineageError {
    /// The manifest store returned an error.
    #[error("manifest store error: {0}")]
    Store(String),
}
