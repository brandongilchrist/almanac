//! Schedule ingestion: translate `KIND_WORKFLOW_DEF` (30620) → [`Schedule`].
//!
//! The expected tag shape (after studying `buzz-workflow`'s def events):
//!
//! ```text
//! kind: 30620
//! tags:
//!   ["d",                 <schedule_id>]
//!   ["name",              <summary>]
//!   ["description",       <description>]      // optional; falls back to content
//!   ["cron",              "<cron-expr>"]        // OR
//!   ["rrule",             "<RFC5545 RRULE>"]    // preferred when present
//!   ["community",         <community_id>]
//!   ["channel",           <channel_id>]         // optional
//!   ["calendar_group",    <group>]              // optional
//!   ["color",             <css color>]          // optional
//!   ["dtstart",           <unix seconds>]       // optional; default = created_at
//! ```
//!
//! If both `cron` and `rrule` are present, `rrule` wins. If only `cron` is
//! present, it is converted to an RRULE via [`cron_to_rrule`]. If neither is
//! present, the schedule is treated as webhook-triggered (empty `rrule`).

use crate::error::IngestError;
use crate::model::Schedule;
use crate::source::SourceEvent;

/// The workflow-def kind emitted by `buzz-workflow`.
pub const KIND_WORKFLOW_DEF: u32 = 30620;

/// Translate a `KIND_WORKFLOW_DEF` event into a [`Schedule`].
pub fn schedule_from_workflow_def(event: &SourceEvent) -> Result<Schedule, IngestError> {
    if event.kind != KIND_WORKFLOW_DEF {
        return Err(IngestError::UnexpectedKind { kind: event.kind });
    }

    let schedule_id = event
        .first_tag_value("d")
        .map(str::to_owned)
        .ok_or(IngestError::MissingTag { tag: "d" })?;

    let summary = event
        .first_tag_value("name")
        .or_else(|| event.first_tag_value("summary"))
        .map(str::to_owned)
        .ok_or(IngestError::MissingTag { tag: "name" })?;

    let description = event
        .first_tag_value("description")
        .map(str::to_owned)
        .unwrap_or_else(|| event.content.clone());

    let rrule = resolve_rrule(event)?;
    let dtstart = event
        .first_tag_value("dtstart")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(event.created_at);

    let community_id = event
        .first_tag_value("community")
        .map(str::to_owned)
        .ok_or(IngestError::MissingTag { tag: "community" })?;

    let channel_id = event
        .first_tag_value("channel")
        .map(str::to_owned)
        .unwrap_or_default();

    let calendar_group = event
        .first_tag_value("calendar_group")
        .map(str::to_owned)
        .unwrap_or_else(|| "default".into());

    let color_category = event.first_tag_value("color").map(str::to_owned);

    let owner_agent_id = event.first_tag_value("agent").map(str::to_owned);

    Ok(Schedule {
        schedule_id,
        community_id,
        channel_id,
        summary,
        description,
        rrule,
        dtstart,
        calendar_group,
        color_category,
        owner_agent_id,
        created_at: event.created_at,
        updated_at: event.created_at,
    })
}

/// Resolve the RRULE for a schedule from its tags. Empty if webhook-only.
fn resolve_rrule(event: &SourceEvent) -> Result<String, IngestError> {
    if let Some(rrule) = event.first_tag_value("rrule") {
        return Ok(rrule.to_owned());
    }
    if let Some(cron) = event.first_tag_value("cron") {
        return Ok(cron_to_rrule(cron));
    }
    // No recurrence → webhook-triggered one-off.
    Ok(String::new())
}

/// Best-effort 5-field cron → RFC 5545 RRULE translation.
///
/// Covers the common cases Almanac cares about:
/// - `M H * * *`    → `FREQ=DAILY;BYHOUR=H;BYMINUTE=M`
/// - `M H * * 0-6`  → `FREQ=WEEKLY;BYDAY=<...>;BYHOUR=H;BYMINUTE=M`
/// - `0 H 1 * *`    → `FREQ=MONTHLY;BYMONTHDAY=1;BYHOUR=H`
///
/// Unknown patterns fall back to a daily rule at the parsed hour/minute so
/// the calendar always shows *something*; callers wanting exact semantics
/// should emit an `rrule` tag directly.
pub fn cron_to_rrule(cron: &str) -> String {
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() < 5 {
        return "FREQ=DAILY".to_string();
    }
    let minute: u32 = parts[0].parse().unwrap_or(0);
    let hour: u32 = parts[1].parse().unwrap_or(0);
    // Cron fields: minute hour day-of-month month day-of-week.
    let dom = parts[2];
    let month = parts[3];
    let dow = parts[4];
    let _ = month; // month is not mapped to RRULE (rare in practice)

    // A specific day-of-month (and not a wildcard) → monthly.
    if dom != "*" && dow == "*" {
        let day: u32 = dom.parse().unwrap_or(1);
        return format!("FREQ=MONTHLY;BYMONTHDAY={day};BYHOUR={hour};BYMINUTE={minute}");
    }

    // Specific days of week → weekly.
    if dow != "*" {
        let byday = dow
            .split(',')
            .filter_map(|d| {
                let d = d.trim();
                match d {
                    "0" | "7" => Some("SU"),
                    "1" => Some("MO"),
                    "2" => Some("TU"),
                    "3" => Some("WE"),
                    "4" => Some("TH"),
                    "5" => Some("FR"),
                    "6" => Some("SA"),
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        if !byday.is_empty() {
            return format!("FREQ=WEEKLY;BYDAY={byday};BYHOUR={hour};BYMINUTE={minute}");
        }
    }

    format!("FREQ=DAILY;BYHOUR={hour};BYMINUTE={minute}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SourceEvent, Tag};

    fn def(tags: Vec<Tag>) -> SourceEvent {
        SourceEvent {
            kind: KIND_WORKFLOW_DEF,
            content: "fallback description".into(),
            tags,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn parses_standard_daily_def() {
        let e = def(vec![
            vec!["d".into(), "daily-brief".into()],
            vec!["name".into(), "Daily research brief".into()],
            vec![
                "description".into(),
                "Produces a brief every morning.".into(),
            ],
            vec!["rrule".into(), "FREQ=DAILY;BYHOUR=9".into()],
            vec!["community".into(), "research".into()],
            vec!["channel".into(), "chan-1".into()],
            vec!["calendar_group".into(), "research".into()],
            vec!["color".into(), "#3b82f6".into()],
        ]);
        let s = schedule_from_workflow_def(&e).unwrap();
        assert_eq!(s.schedule_id, "daily-brief");
        assert_eq!(s.summary, "Daily research brief");
        assert_eq!(s.rrule, "FREQ=DAILY;BYHOUR=9");
        assert_eq!(s.community_id, "research");
        assert_eq!(s.channel_id, "chan-1");
        assert_eq!(s.calendar_group, "research");
        assert_eq!(s.color_category.as_deref(), Some("#3b82f6"));
    }

    #[test]
    fn parses_weekly_cron_def() {
        let e = def(vec![
            vec!["d".into(), "weekly-strat".into()],
            vec!["name".into(), "Weekly strategy".into()],
            vec!["cron".into(), "0 9 * * 1".into()],
            vec!["community".into(), "research".into()],
        ]);
        let s = schedule_from_workflow_def(&e).unwrap();
        assert_eq!(s.rrule, "FREQ=WEEKLY;BYDAY=MO;BYHOUR=9;BYMINUTE=0");
    }

    #[test]
    fn parses_webhook_def_with_no_recurrence() {
        let e = def(vec![
            vec!["d".into(), "on-merge".into()],
            vec!["name".into(), "On PR merge".into()],
            vec!["community".into(), "research".into()],
        ]);
        let s = schedule_from_workflow_def(&e).unwrap();
        assert_eq!(s.rrule, "", "webhook def has empty rrule");
    }

    #[test]
    fn description_falls_back_to_content() {
        let e = def(vec![
            vec!["d".into(), "x".into()],
            vec!["name".into(), "X".into()],
            vec!["rrule".into(), "FREQ=DAILY".into()],
            vec!["community".into(), "c".into()],
        ]);
        let s = schedule_from_workflow_def(&e).unwrap();
        assert_eq!(s.description, "fallback description");
    }

    #[test]
    fn rejects_wrong_kind() {
        let mut e = def(vec![]);
        e.kind = 99999;
        assert!(matches!(
            schedule_from_workflow_def(&e),
            Err(IngestError::UnexpectedKind { .. })
        ));
    }

    #[test]
    fn rejects_missing_d_tag() {
        let e = def(vec![
            vec!["name".into(), "X".into()],
            vec!["community".into(), "c".into()],
        ]);
        assert!(matches!(
            schedule_from_workflow_def(&e),
            Err(IngestError::MissingTag { tag: "d" })
        ));
    }

    #[test]
    fn cron_to_rrule_monthly() {
        assert_eq!(
            cron_to_rrule("0 9 1 * *"),
            "FREQ=MONTHLY;BYMONTHDAY=1;BYHOUR=9;BYMINUTE=0"
        );
    }

    #[test]
    fn cron_to_rrule_multi_day_weekly() {
        assert_eq!(
            cron_to_rrule("30 8 * * 1,3,5"),
            "FREQ=WEEKLY;BYDAY=MO,WE,FR;BYHOUR=8;BYMINUTE=30"
        );
    }

    #[test]
    fn cron_to_rrule_daily_default() {
        assert_eq!(
            cron_to_rrule("15 9 * * *"),
            "FREQ=DAILY;BYHOUR=9;BYMINUTE=15"
        );
    }

    #[test]
    fn cron_to_rrule_garbage_falls_back() {
        assert_eq!(cron_to_rrule("not a cron"), "FREQ=DAILY");
    }
}
