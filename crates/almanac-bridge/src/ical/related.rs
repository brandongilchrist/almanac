//! Lineage rendering — `RELATED-TO;RELTYPE=DEPENDS-ON` (RFC 9253).
//!
//! The renderer is **pure**: it consumes pre-computed [`Dependency`] verdicts
//! from [`crate::lineage::check_inputs`] and emits the corresponding ICS
//! properties. It does not query any store.

use icalendar::{Component, Event, Property};

/// A rendered dependency (re-exported shape from the lineage engine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// The schedule producing the consumed artifact (for RELATED-TO target).
    pub producer_schedule_id: String,
    /// The schema being consumed.
    pub schema_id: String,
    /// Whether it's satisfied.
    pub satisfied: Satisfies,
}

/// Satisfaction state (re-exported shape from the lineage engine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Satisfies {
    /// A fresh, version-matching manifest exists.
    Ready { materialized_at: i64, version: u32 },
    /// No manifest within the freshness window.
    Missing,
    /// A manifest exists but its version is too old.
    VersionMismatch { found: u32, need: u32 },
}

impl From<&crate::lineage::Satisfies> for Satisfies {
    fn from(s: &crate::lineage::Satisfies) -> Self {
        match s {
            crate::lineage::Satisfies::Ready {
                manifest_at,
                version,
            } => Satisfies::Ready {
                materialized_at: *manifest_at,
                version: *version,
            },
            crate::lineage::Satisfies::Missing => Satisfies::Missing,
            crate::lineage::Satisfies::VersionMismatch { found, need } => {
                Satisfies::VersionMismatch {
                    found: *found,
                    need: *need,
                }
            }
        }
    }
}

/// Render a dependency onto a VEVENT.
///
/// - Adds `RELATED-TO;RELTYPE=DEPENDS-ON:<producer_schedule_id>@almanac`.
/// - Appends a "Dependencies:" line to `DESCRIPTION`:
///   - ✅ `<schema_id>` (v`<version>`, materialized `<time>`)
///   - ❌ `<schema_id>` (no manifest in freshness window)
///   - ⚠️ `<schema_id>` (v`<found>` found, need v`<need>`+)
pub fn render_dependency(event: &mut Event, deps: &[Dependency]) {
    if deps.is_empty() {
        return;
    }

    let mut description_lines = Vec::new();
    for d in deps {
        // RELATED-TO;RELTYPE=DEPENDS-ON:<uid>
        // There may be many RELATED-TO per event, so use append_multi_property.
        let uid = format!("{}@almanac", d.producer_schedule_id);
        let related = Property::new("RELATED-TO", &uid)
            .append_parameter(("RELTYPE", "DEPENDS-ON"))
            .done();
        event.append_multi_property(related);

        let line = match &d.satisfied {
            Satisfies::Ready {
                materialized_at,
                version,
            } => {
                let t = chrono::DateTime::<chrono::Utc>::from_timestamp(*materialized_at, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| materialized_at.to_string());
                format!("✅ {} (v{version}, materialized {t})", d.schema_id)
            }
            Satisfies::Missing => {
                format!("❌ {} (no manifest in freshness window)", d.schema_id)
            }
            Satisfies::VersionMismatch { found, need } => {
                format!("⚠️ {} (v{found} found, need v{need}+)", d.schema_id)
            }
        };
        description_lines.push(line);
    }

    let block = format!("\n\nDependencies:\n{}", description_lines.join("\n"));
    append_to_description(event, &block);
}

/// Append `extra` to the event's DESCRIPTION property (creating it if absent).
fn append_to_description(event: &mut Event, extra: &str) {
    let existing = event
        .property_value("DESCRIPTION")
        .map(str::to_owned)
        .unwrap_or_default();
    let combined = format!("{existing}{extra}");
    event.add_property("DESCRIPTION", &combined);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ical::event::render_schedule_vevent;
    use crate::model::Schedule;
    use icalendar::Calendar;
    use std::str::FromStr;

    fn sched() -> Schedule {
        Schedule {
            schedule_id: "weekly-strat".into(),
            community_id: "research".into(),
            channel_id: "chan-2".into(),
            summary: "Weekly strategy".into(),
            description: "Reads the daily brief.".into(),
            rrule: "FREQ=WEEKLY;BYDAY=MO".into(),
            dtstart: 1_700_000_000,
            calendar_group: "research".into(),
            color_category: None,
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
        }
    }

    fn render_to_string(event: Event) -> String {
        let mut cal = Calendar::new();
        cal.push(event);
        cal.to_string()
    }

    /// Unfold RFC 5545 line-folding so substring assertions are robust.
    fn unfold(s: &str) -> String {
        s.replace("\r\n ", "")
            .replace("\r\n\t", "")
            .replace("\n ", "")
            .replace("\n\t", "")
    }

    #[test]
    fn ready_renders_green_check() {
        let mut e = render_schedule_vevent(&sched()).unwrap();
        render_dependency(
            &mut e,
            &[Dependency {
                producer_schedule_id: "daily-brief".into(),
                schema_id: "research-brief".into(),
                satisfied: Satisfies::Ready {
                    materialized_at: 1_700_003_900,
                    version: 3,
                },
            }],
        );
        let out = render_to_string(e);
        let flat = unfold(&out);
        assert!(flat.contains("RELATED-TO;RELTYPE=DEPENDS-ON:daily-brief@almanac"));
        // Commas inside DESCRIPTION values are escaped per RFC 5545.
        assert!(flat.contains("✅ research-brief (v3\\, materialized"));
        assert!(Calendar::from_str(&out).is_ok());
    }

    #[test]
    fn missing_renders_red_x() {
        let mut e = render_schedule_vevent(&sched()).unwrap();
        render_dependency(
            &mut e,
            &[Dependency {
                producer_schedule_id: "daily-brief".into(),
                schema_id: "research-brief".into(),
                satisfied: Satisfies::Missing,
            }],
        );
        let out = render_to_string(e);
        let flat = unfold(&out);
        assert!(flat.contains("❌ research-brief (no manifest"));
    }

    #[test]
    fn version_mismatch_renders_warning() {
        let mut e = render_schedule_vevent(&sched()).unwrap();
        render_dependency(
            &mut e,
            &[Dependency {
                producer_schedule_id: "daily-brief".into(),
                schema_id: "research-brief".into(),
                satisfied: Satisfies::VersionMismatch { found: 2, need: 7 },
            }],
        );
        let out = render_to_string(e);
        let flat = unfold(&out);
        assert!(flat.contains("⚠️ research-brief (v2 found\\, need v7+)"));
    }

    #[test]
    fn no_deps_is_noop() {
        let mut e = render_schedule_vevent(&sched()).unwrap();
        let before = render_to_string(e.clone());
        render_dependency(&mut e, &[]);
        let after = render_to_string(e);
        assert_eq!(before, after);
    }
}
