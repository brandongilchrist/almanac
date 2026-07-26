//! Run ingestion: fire-claim → [`Run`] with status, plus a state machine
//! that advances a [`Run`] through its lifecycle.
//!
//! ## State machine
//!
//! ```text
//! Pending ──start──→ Running ──ok──→ Succeeded
//!                       │
//!                       └──err──→ Failed
//! Pending ──skip──→ Skipped(reason)
//! ```
//!
//! Transitions are driven by [`RunSignal`]s observed on agent output /
//! completion events.

use crate::error::IngestError;
use crate::model::{Run, RunStatus, SkipReason};
use crate::source::SourceEvent;

/// The kind emitted by `buzz-workflow`'s durable fire-claim store.
pub const KIND_WORKFLOW_FIRE: u32 = 46001;

/// A normalized signal extracted from an observed event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunSignal {
    /// Agent started executing (set `started_at`).
    Started { at: i64 },
    /// Agent finished successfully (set `finished_at`, status → Succeeded).
    Succeeded { at: i64 },
    /// Agent failed (set `finished_at`, status → Failed, capture error).
    Failed { at: i64, error: String },
    /// Run skipped because an input wasn't ready.
    Skipped { at: i64, reason: SkipReason },
}

/// Build a fresh `Run` (status `Pending`) from a workflow fire-claim event.
///
/// Expected tag shape:
///
/// ```text
/// kind: 46001
/// tags:
///   ["d",            <run_id>]
///   ["schedule",     <schedule_id>]
///   ["scheduled_for", <unix seconds>]
/// ```
pub fn run_from_fire_claim(event: &SourceEvent) -> Result<Run, IngestError> {
    if event.kind != KIND_WORKFLOW_FIRE {
        return Err(IngestError::UnexpectedKind { kind: event.kind });
    }
    let run_id = event
        .first_tag_value("d")
        .map(str::to_owned)
        .ok_or(IngestError::MissingTag { tag: "d" })?;
    let schedule_id = event
        .first_tag_value("schedule")
        .map(str::to_owned)
        .ok_or(IngestError::MissingTag { tag: "schedule" })?;
    let scheduled_for = event
        .first_tag_value("scheduled_for")
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or(IngestError::MissingTag {
            tag: "scheduled_for",
        })?;

    Ok(Run {
        run_id,
        schedule_id,
        scheduled_for,
        started_at: None,
        finished_at: None,
        status: RunStatus::Pending,
        error: None,
    })
}

/// Apply a [`RunSignal`] to a [`Run`], advancing its lifecycle.
///
/// Returns `Err([IllegalTransition][IngestError::IllegalTransition])` for
/// transitions the state machine forbids (e.g. `Succeeded → Running`).
pub fn transition(run: &mut Run, signal: RunSignal) -> Result<(), IngestError> {
    let from = format!("{:?}", run.status);
    match signal {
        RunSignal::Started { at } => match run.status {
            RunStatus::Pending | RunStatus::Running => {
                run.status = RunStatus::Running;
                run.started_at = Some(at);
                Ok(())
            }
            _ => Err(IngestError::IllegalTransition {
                from,
                to: "Running".into(),
            }),
        },
        RunSignal::Succeeded { at } => match run.status {
            RunStatus::Pending | RunStatus::Running => {
                run.status = RunStatus::Succeeded;
                if run.started_at.is_none() {
                    run.started_at = Some(at);
                }
                run.finished_at = Some(at);
                Ok(())
            }
            _ => Err(IngestError::IllegalTransition {
                from,
                to: "Succeeded".into(),
            }),
        },
        RunSignal::Failed { at, error } => match run.status {
            RunStatus::Pending | RunStatus::Running => {
                run.status = RunStatus::Failed;
                if run.started_at.is_none() {
                    run.started_at = Some(at);
                }
                run.finished_at = Some(at);
                run.error = Some(error);
                Ok(())
            }
            _ => Err(IngestError::IllegalTransition {
                from,
                to: "Failed".into(),
            }),
        },
        RunSignal::Skipped { at, reason } => match run.status {
            RunStatus::Pending => {
                run.status = RunStatus::Skipped(reason);
                run.finished_at = Some(at);
                Ok(())
            }
            _ => Err(IngestError::IllegalTransition {
                from,
                to: "Skipped".into(),
            }),
        },
    }
}

/// Best-effort extraction of a [`RunSignal`] from an agent output /
/// completion event. Looks for `status:<value>` and `error:<text>` tags.
///
/// Recognized `status:` values: `running`, `succeeded`/`ok`/`success`,
/// `failed`/`error`, `skipped`.
pub fn signal_from_event(event: &SourceEvent, default_at: i64) -> Option<RunSignal> {
    let status = event.first_typed_value("status")?;
    let at = event
        .first_typed_value("at")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default_at);
    match status {
        "running" | "started" => Some(RunSignal::Started { at }),
        "succeeded" | "ok" | "success" => Some(RunSignal::Succeeded { at }),
        "failed" | "error" => Some(RunSignal::Failed {
            at,
            error: event
                .first_typed_value("error")
                .map(str::to_owned)
                .unwrap_or_else(|| "agent error".into()),
        }),
        "skipped" => {
            let schema = event
                .first_typed_value("missing_input")
                .map(str::to_owned)
                .unwrap_or_default();
            let version_mismatch = event
                .first_typed_value("version_mismatch")
                .map(str::to_owned);
            let reason = match version_mismatch {
                Some(v) => SkipReason::VersionMismatch(v),
                None => SkipReason::MissingInput(schema),
            };
            Some(RunSignal::Skipped { at, reason })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Tag;

    fn fire(tags: Vec<Tag>) -> SourceEvent {
        SourceEvent {
            kind: KIND_WORKFLOW_FIRE,
            content: String::new(),
            tags,
            created_at: 1_700_000_000,
        }
    }

    fn fresh_run() -> Run {
        run_from_fire_claim(&fire(vec![
            vec!["d".into(), "run-1".into()],
            vec!["schedule".into(), "daily-brief".into()],
            vec!["scheduled_for".into(), "1700003600".into()],
        ]))
        .unwrap()
    }

    #[test]
    fn fire_claim_yields_pending_run() {
        let r = fresh_run();
        assert_eq!(r.run_id, "run-1");
        assert_eq!(r.schedule_id, "daily-brief");
        assert_eq!(r.scheduled_for, 1_700_003_600);
        assert_eq!(r.status, RunStatus::Pending);
        assert!(r.started_at.is_none());
    }

    #[test]
    fn legal_transitions() {
        let mut r = fresh_run();
        transition(&mut r, RunSignal::Started { at: 10 }).unwrap();
        assert_eq!(r.status, RunStatus::Running);
        assert_eq!(r.started_at, Some(10));

        transition(&mut r, RunSignal::Succeeded { at: 20 }).unwrap();
        assert_eq!(r.status, RunStatus::Succeeded);
        assert_eq!(r.finished_at, Some(20));
    }

    #[test]
    fn pending_can_succeed_directly() {
        let mut r = fresh_run();
        transition(&mut r, RunSignal::Succeeded { at: 30 }).unwrap();
        assert_eq!(r.status, RunStatus::Succeeded);
        // backfilled started_at to the success time
        assert_eq!(r.started_at, Some(30));
    }

    #[test]
    fn failed_captures_error() {
        let mut r = fresh_run();
        transition(&mut r, RunSignal::Started { at: 10 }).unwrap();
        transition(
            &mut r,
            RunSignal::Failed {
                at: 15,
                error: "exit 1".into(),
            },
        )
        .unwrap();
        assert_eq!(r.status, RunStatus::Failed);
        assert_eq!(r.error.as_deref(), Some("exit 1"));
    }

    #[test]
    fn skipped_only_from_pending() {
        let mut r = fresh_run();
        transition(
            &mut r,
            RunSignal::Skipped {
                at: 5,
                reason: SkipReason::MissingInput("research-brief".into()),
            },
        )
        .unwrap();
        assert_eq!(
            r.status,
            RunStatus::Skipped(SkipReason::MissingInput("research-brief".into()))
        );
    }

    #[test]
    fn illegal_succeeded_to_running() {
        let mut r = fresh_run();
        transition(&mut r, RunSignal::Succeeded { at: 30 }).unwrap();
        let err = transition(&mut r, RunSignal::Started { at: 40 }).unwrap_err();
        assert!(matches!(err, IngestError::IllegalTransition { .. }));
    }

    #[test]
    fn illegal_skipped_after_started() {
        let mut r = fresh_run();
        transition(&mut r, RunSignal::Started { at: 10 }).unwrap();
        let err = transition(
            &mut r,
            RunSignal::Skipped {
                at: 11,
                reason: SkipReason::MissingInput("x".into()),
            },
        )
        .unwrap_err();
        assert!(matches!(err, IngestError::IllegalTransition { .. }));
    }

    #[test]
    fn signal_from_event_parses_all_variants() {
        let mk = |tags: Vec<Tag>| SourceEvent {
            kind: 0,
            content: String::new(),
            tags,
            created_at: 99,
        };
        let s = signal_from_event(&mk(vec![vec!["status".into(), "running".into()]]), 100).unwrap();
        assert_eq!(s, RunSignal::Started { at: 100 });

        let s =
            signal_from_event(&mk(vec![vec!["status".into(), "succeeded".into()]]), 100).unwrap();
        assert_eq!(s, RunSignal::Succeeded { at: 100 });

        let s = signal_from_event(
            &mk(vec![
                vec!["status".into(), "failed".into()],
                vec!["error".into(), "boom".into()],
            ]),
            100,
        )
        .unwrap();
        assert_eq!(
            s,
            RunSignal::Failed {
                at: 100,
                error: "boom".into()
            }
        );

        let s = signal_from_event(
            &mk(vec![
                vec!["status".into(), "skipped".into()],
                vec!["missing_input".into(), "research-brief".into()],
            ]),
            100,
        )
        .unwrap();
        assert_eq!(
            s,
            RunSignal::Skipped {
                at: 100,
                reason: SkipReason::MissingInput("research-brief".into())
            }
        );

        let s = signal_from_event(
            &mk(vec![
                vec!["status".into(), "skipped".into()],
                vec!["version_mismatch".into(), "research-brief".into()],
            ]),
            100,
        )
        .unwrap();
        assert!(matches!(
            s,
            RunSignal::Skipped {
                reason: SkipReason::VersionMismatch(_),
                ..
            }
        ));
    }

    #[test]
    fn signal_from_event_unknown_status_is_none() {
        let e = SourceEvent {
            kind: 0,
            content: String::new(),
            tags: vec![vec!["status".into(), "wat".into()]],
            created_at: 99,
        };
        assert!(signal_from_event(&e, 100).is_none());
    }
}
