//! Calendar feed rendering — an ordered list of VEVENTs + `X-WR-CALNAME`.

use crate::error::IcalError;
use crate::ical::event::{render_run_vevent, render_schedule_vevent};
use crate::ical::related::{render_dependency, Dependency as RenderDependency, Satisfies};
use crate::lineage::Dependency;
use crate::model::{Calendar as CalendarMeta, Run, Schedule};
use icalendar::{Calendar, CalendarComponent};
use std::collections::HashMap;

/// Which slice of the calendar to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeedFilter {
    /// Default: recurring schedules with today's run state overlaid.
    #[default]
    Schedule,
    /// One-off events per concrete run (no RRULE).
    Runs,
}

/// Render a full ICS feed string for a set of schedules, runs, calendars,
/// and pre-computed lineage dependencies.
///
/// - `schedules` — the recurring plans.
/// - `runs` — map from `schedule_id` → latest run (for status overlay).
/// - `deps` — map from `schedule_id` → dependency verdicts (for RELATED-TO).
/// - `calendars` — calendars in scope (first one's name becomes X-WR-CALNAME).
/// - `filter` — which slice to render.
/// - `community_name` — display name if no calendars given.
pub fn render_calendar_feed(
    schedules: &[Schedule],
    runs: &HashMap<String, Run>,
    deps: &HashMap<String, Vec<Dependency>>,
    calendars: &[CalendarMeta],
    filter: FeedFilter,
    community_name: &str,
) -> Result<String, IcalError> {
    let mut cal = Calendar::new();
    cal.name(
        calendars
            .first()
            .map(|c| c.name.as_str())
            .unwrap_or(community_name),
    );
    cal.append_property(icalendar::Property::new(
        "X-WR-CALDESC",
        "Agent schedules and artifact lineage — rendered by Almanac.",
    ));
    cal.append_property(icalendar::Property::new("X-WR-TIMEZONE", "UTC"));

    match filter {
        FeedFilter::Schedule => {
            for s in schedules {
                let mut event = render_schedule_vevent(s)?;
                if let Some(run) = runs.get(&s.schedule_id) {
                    crate::ical::event::overlay_run_status(&mut event, Some(run));
                }
                // Render lineage if this schedule has consuming dependencies.
                if let Some(ds) = deps.get(&s.schedule_id) {
                    let render_deps: Vec<RenderDependency> = ds
                        .iter()
                        .filter_map(|d| {
                            d.producer_schedule_id.as_ref().map(|pid| RenderDependency {
                                producer_schedule_id: pid.clone(),
                                schema_id: d.schema_id.clone(),
                                satisfied: Satisfies::from(&d.satisfied),
                            })
                        })
                        .collect();
                    render_dependency(&mut event, &render_deps);
                }
                cal.push(CalendarComponent::Event(event));
            }
        }
        FeedFilter::Runs => {
            for s in schedules {
                if let Some(run) = runs.get(&s.schedule_id) {
                    let event = render_run_vevent(run, &s.summary)?;
                    cal.push(CalendarComponent::Event(event));
                }
            }
        }
    }

    Ok(cal.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RunStatus, Schedule};
    use icalendar::Calendar as IcalCalendar;
    use std::str::FromStr;

    fn sched(id: &str) -> Schedule {
        Schedule {
            schedule_id: id.into(),
            community_id: "research".into(),
            channel_id: "c".into(),
            summary: format!("Summary {id}"),
            description: "desc".into(),
            rrule: "FREQ=DAILY;BYHOUR=9".into(),
            dtstart: 1_700_000_000,
            calendar_group: "research".into(),
            color_category: None,
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
        }
    }

    #[test]
    fn schedule_feed_has_two_events_and_calname() {
        let schedules = vec![sched("a"), sched("b")];
        let feed = render_calendar_feed(
            &schedules,
            &HashMap::new(),
            &HashMap::new(),
            &[],
            FeedFilter::Schedule,
            "research",
        )
        .unwrap();
        assert!(feed.contains("X-WR-CALNAME:research"));
        assert!(feed.contains("UID:a@almanac"));
        assert!(feed.contains("UID:b@almanac"));
        assert!(IcalCalendar::from_str(&feed).is_ok());
    }

    #[test]
    fn runs_feed_only_includes_runs() {
        let schedules = vec![sched("a"), sched("b")];
        let mut runs = HashMap::new();
        runs.insert(
            "a".into(),
            Run {
                run_id: "r1".into(),
                schedule_id: "a".into(),
                scheduled_for: 1_700_000_000,
                started_at: Some(1_700_000_010),
                finished_at: Some(1_700_000_020),
                status: RunStatus::Succeeded,
                error: None,
            },
        );
        let feed = render_calendar_feed(
            &schedules,
            &runs,
            &HashMap::new(),
            &[],
            FeedFilter::Runs,
            "research",
        )
        .unwrap();
        assert!(feed.contains("r1@almanac.run"));
        assert!(!feed.contains("UID:b@almanac"));
    }

    #[test]
    fn calendar_meta_provides_calname() {
        let schedules = vec![sched("a")];
        let cal = CalendarMeta {
            calendar_id: "c1".into(),
            community_id: "research".into(),
            name: "Research Schedules".into(),
            description: "all".into(),
            color: None,
            schedule_ids: vec!["a".into()],
        };
        let feed = render_calendar_feed(
            &schedules,
            &HashMap::new(),
            &HashMap::new(),
            &[cal],
            FeedFilter::Schedule,
            "ignored",
        )
        .unwrap();
        assert!(feed.contains("X-WR-CALNAME:Research Schedules"));
    }
}
