//! VEVENT rendering — recurring schedule + status overlay.
//!
//! Field mapping per `10_PLAN.md` § "The ICS rendering contract":
//!
//! | Concept        | iCalendar field                              |
//! |----------------|----------------------------------------------|
//! | Schedule       | `VEVENT` + `RRULE`                           |
//! | Schedule name  | `SUMMARY` (emoji-prefixed when run overlays) |
//! | Description    | `DESCRIPTION`                               |
//! | Color          | `CATEGORIES` (`almanac:<group>`)             |
//! | Run state      | `STATUS`                                     |
//! | Run reason     | `DESCRIPTION` append                         |

use crate::error::IcalError;
use crate::model::{Run, RunStatus, Schedule, SkipReason};
use chrono::{DateTime, Utc};
use icalendar::{Component, Event, EventLike, EventStatus};

/// Canonical UID format: `<schedule_id>@almanac`.
pub fn uid_for(schedule_id: &str) -> String {
    format!("{schedule_id}@almanac")
}

/// Map a [`RunStatus`] to an iCalendar `STATUS` value (RFC 5545).
///
/// `Pending`/`Running` → `TENTATIVE`, `Succeeded` → `CONFIRMED`,
/// `Failed`/`Skipped` → `CANCELLED`.
pub fn status_to_ical(status: &RunStatus) -> EventStatus {
    match status {
        RunStatus::Pending | RunStatus::Running => EventStatus::Tentative,
        RunStatus::Succeeded => EventStatus::Confirmed,
        RunStatus::Failed | RunStatus::Skipped(_) => EventStatus::Cancelled,
    }
}

/// The emoji prefix for a [`RunStatus`], per `10_PLAN.md`'s legend.
///
/// - 🟡 pending
/// - ⏳ running
/// - ✅ succeeded
/// - ❌ failed
/// - ⏸ skipped
pub fn status_emoji(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Pending => "🟡",
        RunStatus::Running => "⏳",
        RunStatus::Succeeded => "✅",
        RunStatus::Failed => "❌",
        RunStatus::Skipped(_) => "⏸",
    }
}

fn unix_to_dt(ts: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
}

/// Render a [`Schedule`] into a recurring `VEVENT`.
///
/// Sets `UID`, `SUMMARY`, `DESCRIPTION`, `DTSTART`, `RRULE` (if any),
/// `CATEGORIES`, and a default `STATUS:TENTATIVE`.
///
/// The returned `Event` can be further mutated by [`overlay_run_status`]
/// and [`crate::ical::related::render_dependency`].
pub fn render_schedule_vevent(schedule: &Schedule) -> Result<Event, IcalError> {
    let uid = uid_for(&schedule.schedule_id);
    let dtstart = unix_to_dt(schedule.dtstart);

    let mut event = Event::new();
    event.uid(&uid);
    event.summary(&schedule.summary);
    event.description(&schedule.description);
    event.starts(dtstart);
    event.status(EventStatus::Tentative);
    event.timestamp(unix_to_dt(schedule.updated_at));
    // CATEGORIES:almanac,<group>
    event.add_property("CATEGORIES", format!("almanac,{}", schedule.calendar_group));

    // RRULE if present (webhook schedules have none → one-off VEVENT).
    if !schedule.rrule.trim().is_empty() {
        validate_rrule(&schedule.rrule)?;
        event.add_property("RRULE", &schedule.rrule);
    }

    // DTEND: default 30 minutes after DTSTART so clients render a block.
    let dtend = dtstart + chrono::Duration::minutes(30);
    event.ends(dtend);

    Ok(event.done())
}

/// Overlay a [`Run`]'s state onto a rendered VEVENT.
///
/// - Sets `STATUS` per [`status_to_ical`].
/// - Prefixes `SUMMARY` with the status emoji.
/// - Appends the run state + error reason to `DESCRIPTION`.
pub fn overlay_run_status(event: &mut Event, run: Option<&Run>) {
    let Some(run) = run else {
        return; // no run yet → schedule defaults (TENTATIVE, no emoji)
    };

    let status = status_to_ical(&run.status);
    let emoji = status_emoji(&run.status);

    // Rewrite SUMMARY with emoji prefix.
    let current_summary = event
        .property_value("SUMMARY")
        .map(str::to_owned)
        .unwrap_or_default();
    let bare = strip_leading_emoji(&current_summary);
    let new_summary = format!("{emoji} {bare}");
    replace_property(event, "SUMMARY", &new_summary);

    // STATUS
    event.status(status);

    // DESCRIPTION append.
    let note = run_note(run);
    append_to_description(event, &note);
}

/// Render a one-off VEVENT for a concrete [`Run`] (used on `/runs.ics`).
///
/// `DTSTART` = run's effective time (`started_at` or `scheduled_for`).
pub fn render_run_vevent(run: &Run, schedule_summary: &str) -> Result<Event, IcalError> {
    let uid = format!("{}@almanac.run", run.run_id);
    let ts = run.started_at.unwrap_or(run.scheduled_for);
    let start = unix_to_dt(ts);
    let end = start + chrono::Duration::minutes(30);

    let mut event = Event::new();
    event.uid(&uid);
    event.summary(schedule_summary);
    event.starts(start);
    event.ends(end);
    event.timestamp(unix_to_dt(run.finished_at.unwrap_or(ts)));
    event.add_property("CATEGORIES", "almanac,run");
    overlay_run_status(&mut event, Some(run));
    Ok(event.done())
}

// ---- helpers not provided by the icalendar crate ----

/// Replace the value of the property named `key` on `event`, or add it.
fn replace_property(event: &mut Event, key: &str, value: &str) {
    // `add_property` inserts into the BTreeMap, overwriting any existing
    // value for the same key (icalendar 0.17 Component trait semantics).
    event.add_property(key, value);
}

/// Append `extra` to the event's DESCRIPTION property (creating it if absent).
fn append_to_description(event: &mut Event, extra: &str) {
    let existing = event
        .property_value("DESCRIPTION")
        .map(str::to_owned)
        .unwrap_or_default();
    let combined = format!("{existing}{extra}");
    replace_property(event, "DESCRIPTION", &combined);
}

/// Strip a leading emoji (any non-word, non-space leading prefix up to first space).
fn strip_leading_emoji(s: &str) -> &str {
    if let Some(idx) = s.find(' ') {
        let prefix = &s[..idx];
        // Heuristic: if the prefix is not ASCII-printable word chars, treat as emoji.
        if !prefix.is_ascii() || prefix.is_empty() {
            return s[idx + 1..].trim_start();
        }
    }
    s.trim()
}

fn run_note(run: &Run) -> String {
    let mut note = match &run.status {
        RunStatus::Pending => "Pending: scheduled, has not started.".to_string(),
        RunStatus::Running => "Running: currently executing.".to_string(),
        RunStatus::Succeeded => format!(
            "Succeeded at {}.",
            run.finished_at
                .map(|t| unix_to_dt(t).to_rfc3339())
                .unwrap_or_else(|| "unknown".into())
        ),
        RunStatus::Failed => format!("Failed: {}", run.error.as_deref().unwrap_or("agent error")),
        RunStatus::Skipped(reason) => match reason {
            SkipReason::MissingInput(s) => {
                format!("Skipped: missing input `{s}` within freshness window.")
            }
            SkipReason::VersionMismatch(s) => {
                format!("Skipped: version mismatch for `{s}`.")
            }
        },
    };
    if let Some(err) = &run.error {
        if !matches!(run.status, RunStatus::Failed) {
            note.push_str(&format!(" Note: {err}."));
        }
    }
    note
}

/// Lightweight RRULE sanity check. The `icalendar` crate does not validate
/// RRULE content itself; we check for obviously-malformed strings and let
/// real clients do the rest. Returns `IcalError::InvalidRrule` on garbage.
fn validate_rrule(rrule: &str) -> Result<(), IcalError> {
    let upper = rrule.trim().to_ascii_uppercase();
    if !upper.starts_with("FREQ=") {
        return Err(IcalError::InvalidRrule {
            rrule: rrule.into(),
            reason: "must start with FREQ=".into(),
        });
    }
    let valid_freqs = [
        "FREQ=DAILY",
        "FREQ=WEEKLY",
        "FREQ=MONTHLY",
        "FREQ=YEARLY",
        "FREQ=HOURLY",
        "FREQ=MINUTELY",
    ];
    if !valid_freqs.iter().any(|f| upper.starts_with(f)) {
        return Err(IcalError::InvalidRrule {
            rrule: rrule.into(),
            reason: "unknown FREQ".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Run, RunStatus, Schedule, SkipReason};
    use icalendar::Calendar;
    use std::str::FromStr;

    fn sched() -> Schedule {
        Schedule {
            schedule_id: "daily-brief".into(),
            community_id: "research".into(),
            channel_id: "chan-1".into(),
            summary: "Daily research brief".into(),
            description: "Produces a brief every morning.".into(),
            rrule: "FREQ=DAILY;BYHOUR=9".into(),
            dtstart: 1_700_000_000,
            calendar_group: "research".into(),
            color_category: Some("#3b82f6".into()),
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
        }
    }

    fn run(status: RunStatus) -> Run {
        Run {
            run_id: "run-1".into(),
            schedule_id: "daily-brief".into(),
            scheduled_for: 1_700_003_600,
            started_at: Some(1_700_003_610),
            finished_at: Some(1_700_003_900),
            status,
            error: None,
        }
    }

    fn render_to_string(event: Event) -> String {
        let mut cal = Calendar::new();
        cal.push(event);
        cal.to_string()
    }

    #[test]
    fn renders_schedule_with_rrule_and_categories() {
        let e = render_schedule_vevent(&sched()).unwrap();
        let s = render_to_string(e);
        let flat = unfold(&s);
        assert!(flat.contains("UID:daily-brief@almanac"));
        assert!(flat.contains("SUMMARY:Daily research brief"));
        assert!(flat.contains("RRULE:FREQ=DAILY;BYHOUR=9"));
        // Commas in CATEGORIES values are escaped per RFC 5545.
        assert!(flat.contains("CATEGORIES:almanac\\,research"));
        assert!(flat.contains("STATUS:TENTATIVE"));
        // Parses back.
        assert!(Calendar::from_str(&s).is_ok(), "output must parse");
    }

    /// Unfold RFC 5545 line-folding so substring assertions are robust.
    fn unfold(s: &str) -> String {
        // Folded lines end with CRLF + (space|tab) on the next line.
        s.replace("\r\n ", "")
            .replace("\r\n\t", "")
            .replace("\n ", "")
            .replace("\n\t", "")
    }

    #[test]
    fn webhook_schedule_has_no_rrule() {
        let mut s = sched();
        s.rrule.clear();
        let e = render_schedule_vevent(&s).unwrap();
        let out = render_to_string(e);
        assert!(!out.contains("RRULE"));
    }

    #[test]
    fn overlay_per_status_variant() {
        for st in [
            RunStatus::Pending,
            RunStatus::Running,
            RunStatus::Succeeded,
            RunStatus::Failed,
            RunStatus::Skipped(SkipReason::MissingInput("x".into())),
        ] {
            let mut e = render_schedule_vevent(&sched()).unwrap();
            overlay_run_status(&mut e, Some(&run(st.clone())));
            let out = render_to_string(e);
            let emoji = status_emoji(&st);
            assert!(
                out.contains(&format!("SUMMARY:{emoji}")),
                "missing emoji {emoji} for {st:?} in:\n{out}"
            );
            let want_status = match &st {
                RunStatus::Pending | RunStatus::Running => "TENTATIVE",
                RunStatus::Succeeded => "CONFIRMED",
                RunStatus::Failed | RunStatus::Skipped(_) => "CANCELLED",
            };
            assert!(
                out.contains(&format!("STATUS:{want_status}")),
                "missing STATUS:{want_status} for {st:?}"
            );
            assert!(Calendar::from_str(&out).is_ok(), "output must parse");
        }
    }

    #[test]
    fn overlay_none_leaves_defaults() {
        let mut e = render_schedule_vevent(&sched()).unwrap();
        overlay_run_status(&mut e, None);
        let out = render_to_string(e);
        // No emoji prefix.
        assert!(out.contains("SUMMARY:Daily research brief"));
        assert!(out.contains("STATUS:TENTATIVE"));
    }

    #[test]
    fn uid_format() {
        assert_eq!(uid_for("daily-brief"), "daily-brief@almanac");
    }

    #[test]
    fn invalid_rrule_rejected() {
        let mut s = sched();
        s.rrule = "GARBLY".into();
        assert!(render_schedule_vevent(&s).is_err());

        s.rrule = "FREQ=WAT".into();
        assert!(render_schedule_vevent(&s).is_err());
    }
}
