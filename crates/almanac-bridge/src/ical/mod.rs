//! ICS rendering.
//!
//! - [`event`] — VEVENT construction (RRULE, STATUS, emoji SUMMARY, CATEGORIES).
//! - [`related`] — `RELATED-TO;RELTYPE=DEPENDS-ON` (RFC 9253).
//! - [`feed`] — Calendar == ordered list of VEVENTs + `X-WR-CALNAME`.

pub mod event;
pub mod feed;
pub mod related;

pub use event::{overlay_run_status, render_schedule_vevent, status_emoji, status_to_ical};
pub use feed::{render_calendar_feed, FeedFilter};
pub use related::{
    render_dependency, Dependency as RenderDependency, Satisfies as RenderSatisfies,
};
