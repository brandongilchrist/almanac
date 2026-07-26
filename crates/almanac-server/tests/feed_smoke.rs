//! End-to-end smoke test: emit events, GET /calendar/<community>.ics, parse,
//! and assert on content. Mirrors the spec's T15 checklist.
//!
//! Stands up the full axum router in-process (no external HTTP) using
//! `tower::ServiceExt::oneshot`, seeds a two-schedule lineage scenario,
//! then exercises:
//!
//! 1. The daily schedule shows STATUS:CONFIRMED with a ✅ emoji (it succeeded).
//! 2. The nightly index shows STATUS:CANCELLED with a ❌ emoji (it failed).
//! 3. The weekly strategy's VEVENT contains RELATED-TO;RELTYPE=DEPENDS-ON
//!    pointing at the daily schedule's UID (lineage edge rendered).
//! 4. The weekly strategy's DESCRIPTION contains a ✅ marker for the
//!    consumed artifact (manifest is fresh + version-OK).
//! 5. The runs feed contains one VEVENT per concrete run.
//! 6. Every emitted feed parses with the `icalendar` crate.

use almanac_server::router;
use almanac_server::seed::seed_demo;
use almanac_server::State;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use icalendar::Calendar;
use std::str::FromStr;
use tower::ServiceExt;

const BODY_LIMIT: usize = 1 << 20;

async fn seeded_app() -> axum::Router {
    let state = State::new();
    seed_demo(&state).await;
    router(state)
}

async fn get_feed(app: axum::Router, uri: &str) -> String {
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "for {uri}");
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "text/calendar; charset=utf-8"
    );
    let bytes = to_bytes(res.into_body(), BODY_LIMIT).await.unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// RFC 5545 unfolding: rejoin lines split by CRLF+space.
fn unfold(s: &str) -> String {
    s.replace("\r\n ", "")
        .replace("\r\n\t", "")
        .replace("\n ", "")
        .replace("\n\t", "")
}

#[tokio::test]
async fn full_feed_is_valid_and_has_all_schedules() {
    let app = seeded_app().await;
    let feed = get_feed(app, "/calendar/demo.ics").await;
    let flat = unfold(&feed);

    // Parses.
    assert!(Calendar::from_str(&feed).is_ok(), "feed must parse");

    // All three recurring demo schedules present (the webhook schedule
    // `pr-review` appears on /runs.ics, tested separately).
    assert!(flat.contains("UID:daily-brief@almanac"));
    assert!(flat.contains("UID:weekly-strategy@almanac"));
    assert!(flat.contains("UID:nightly-index@almanac"));
}

#[tokio::test]
async fn succeeded_run_shows_confirmed_and_green_check() {
    let app = seeded_app().await;
    let flat = unfold(&get_feed(app, "/calendar/demo.ics").await);
    assert!(flat.contains("STATUS:CONFIRMED"));
    assert!(flat.contains("SUMMARY:✅ Daily research brief"));
}

#[tokio::test]
async fn failed_run_shows_cancelled_and_red_x() {
    let app = seeded_app().await;
    let flat = unfold(&get_feed(app, "/calendar/demo.ics").await);
    assert!(flat.contains("STATUS:CANCELLED"));
    assert!(flat.contains("SUMMARY:❌ Nightly vector index rebuild"));
    // Error reason surfaced in DESCRIPTION.
    assert!(flat.contains("Failed: index writer OOM"));
}

#[tokio::test]
async fn weekly_strategy_renders_dependency_on_daily_brief() {
    let app = seeded_app().await;
    let flat = unfold(&get_feed(app, "/calendar/demo.ics").await);
    // The weekly strategy declares a consume contract for research-brief,
    // whose producer is daily-brief → RELATED-TO edge + ✅ in DESCRIPTION.
    assert!(
        flat.contains("RELATED-TO;RELTYPE=DEPENDS-ON:daily-brief@almanac"),
        "missing RELATED-TO edge"
    );
    assert!(
        flat.contains("✅ research-brief (v3\\, materialized"),
        "missing ✅ lineage marker in DESCRIPTION"
    );
}

#[tokio::test]
async fn runs_feed_has_concrete_run_events() {
    let app = seeded_app().await;
    let feed = get_feed(app, "/calendar/demo/runs.ics").await;
    let flat = unfold(&feed);
    assert!(Calendar::from_str(&feed).is_ok());
    // Each concrete run becomes a VEVENT with a .run UID.
    assert!(flat.contains("run-daily-001@almanac.run"));
    assert!(flat.contains("run-index-001@almanac.run"));
    assert!(flat.contains("run-pr-042@almanac.run"));
}

#[tokio::test]
async fn unknown_community_yields_empty_valid_feed() {
    let app = seeded_app().await;
    let feed = get_feed(app, "/calendar/ghost-town.ics").await;
    assert!(Calendar::from_str(&feed).is_ok());
    assert!(feed.contains("BEGIN:VCALENDAR"));
    assert!(
        !feed.contains("BEGIN:VEVENT"),
        "no events for unknown community"
    );
}

#[tokio::test]
async fn ingestion_then_feed_reflects_new_state() {
    // Publish a new schedule + run via the ingestion endpoints, then confirm
    // it appears in the feed with the correct status.
    let state = State::new();
    seed_demo(&state).await;
    let app = router(state);

    let sched = serde_json::json!({
        "schedule_id": "hourly-heartbeat",
        "community_id": "demo",
        "channel_id": "demo",
        "summary": "Hourly heartbeat",
        "description": "Health check.",
        "rrule": "FREQ=HOURLY",
        "dtstart": 1700000000,
        "calendar_group": "infra",
        "color_category": null,
        "created_at": 1700000000,
        "updated_at": 1700000000
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules")
                .header("content-type", "application/json")
                .body(Body::from(sched.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let run = serde_json::json!({
        "run_id": "run-heartbeat-1",
        "schedule_id": "hourly-heartbeat",
        "scheduled_for": 1700003600,
        "started_at": 1700003610,
        "finished_at": 1700003620,
        "status": {"kind": "Succeeded"},
        "error": null
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(run.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let feed = get_feed(app, "/calendar/demo.ics").await;
    let flat = unfold(&feed);
    assert!(flat.contains("UID:hourly-heartbeat@almanac"));
    assert!(flat.contains("SUMMARY:✅ Hourly heartbeat"));
    assert!(flat.contains("STATUS:CONFIRMED"));
}
