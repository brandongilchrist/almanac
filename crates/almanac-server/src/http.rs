//! HTTP routes — the Almanac server surface.
//!
//! ## Calendar feeds (the product)
//!
//! - `GET /calendar/<community>.ics` — default feed (schedules + today's run
//!   overlay + lineage). `text/calendar`.
//! - `GET /calendar/<community>/schedule.ics` — recurring schedules only.
//! - `GET /calendar/<community>/runs.ics` — one-off events per concrete run.
//!
//! ## Ingestion (write surface)
//!
//! - `POST /v1/schedules` — upsert a schedule (JSON body).
//! - `POST /v1/runs` — upsert a run.
//! - `POST /v1/contracts` — add a contract.
//! - `POST /v1/manifests` — record a manifest.
//!
//! ## Introspection
//!
//! - `GET /healthz` — liveness.
//! - `GET /v1/communities/<community>/state` — JSON snapshot of state.
//! - `GET /v1/communities/<community>/lineage/<schedule_id>` — lineage verdicts.

use crate::store::State;
use almanac_bridge::ical::{render_calendar_feed, FeedFilter};
use almanac_bridge::lineage::{check_inputs, Dependency};
use almanac_bridge::model::{Calendar, Contract, Manifest, Run, Schedule};
use axum::extract::{Path, State as AppState};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use serde::Serialize;
use std::collections::HashMap;

/// Shared application state.
pub type SharedState = AppState<State>;

/// Build the full axum router.
pub fn router(state: State) -> axum::Router {
    axum::Router::new()
        // Calendar feeds.
        .route("/calendar/:community.ics", get(get_calendar))
        .route("/calendar/:community/schedule.ics", get(get_schedule_feed))
        .route("/calendar/:community/runs.ics", get(get_runs_feed))
        // Ingestion.
        .route("/v1/schedules", post(upsert_schedule))
        .route("/v1/runs", post(upsert_run))
        .route("/v1/contracts", post(add_contract))
        .route("/v1/manifests", post(put_manifest))
        .route("/v1/calendars", post(add_calendar))
        // Introspection.
        .route("/healthz", get(healthz))
        .route("/v1/communities/:community/state", get(get_state))
        .route(
            "/v1/communities/:community/lineage/:schedule_id",
            get(get_lineage),
        )
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

async fn get_calendar(AppState(state): AppState<State>, Path(community): Path<String>) -> Response {
    // The route captures the full segment including the `.ics` suffix;
    // strip it so lookups key on the bare community id.
    let community = community
        .strip_suffix(".ics")
        .unwrap_or(&community)
        .to_string();
    render_feed(&state, &community, FeedFilter::Schedule).await
}

async fn get_schedule_feed(
    AppState(state): AppState<State>,
    Path(community): Path<String>,
) -> Response {
    render_feed(&state, &community, FeedFilter::Schedule).await
}

async fn get_runs_feed(
    AppState(state): AppState<State>,
    Path(community): Path<String>,
) -> Response {
    render_feed(&state, &community, FeedFilter::Runs).await
}

async fn render_feed(state: &State, community: &str, filter: FeedFilter) -> Response {
    let schedules = state.schedules_for(community).await;
    let runs = state.runs_for(community).await;
    let calendars = state.calendars_for(community).await;
    let contracts = state.contracts_for(community).await;

    // Compute lineage deps for each schedule that has consuming contracts.
    let manifest_store = state.manifest_store_async().await;
    let mut deps_map: HashMap<String, Vec<Dependency>> = HashMap::new();
    for sched in &schedules {
        let mine: Vec<Contract> = contracts
            .iter()
            .filter(|c| c.schedule_id == sched.schedule_id)
            .cloned()
            .collect();
        if mine
            .iter()
            .any(|c| c.role == almanac_bridge::model::ContractRole::Consume)
        {
            if let Some(run) = runs.get(&sched.schedule_id) {
                if let Ok(deps) = check_inputs(&manifest_store, run, &mine).await {
                    // Attach producer schedule ids where we can derive them.
                    let enriched: Vec<Dependency> = deps
                        .into_iter()
                        .map(|mut d| {
                            // Find the producer schedule for this schema.
                            d.producer_schedule_id = contracts
                                .iter()
                                .find(|c| {
                                    c.role == almanac_bridge::model::ContractRole::Produce
                                        && c.schema_id == d.schema_id
                                })
                                .map(|c| c.schedule_id.clone());
                            d
                        })
                        .collect();
                    deps_map.insert(sched.schedule_id.clone(), enriched);
                }
            }
        }
    }

    match render_calendar_feed(&schedules, &runs, &deps_map, &calendars, filter, community) {
        Ok(body) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/calendar; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("render error: {e}"),
        )
            .into_response(),
    }
}

async fn upsert_schedule(
    AppState(state): AppState<State>,
    Json(schedule): Json<Schedule>,
) -> Response {
    state.upsert_schedule(schedule).await;
    (StatusCode::CREATED, "created").into_response()
}

async fn upsert_run(AppState(state): AppState<State>, Json(run): Json<Run>) -> Response {
    state.upsert_run(run).await;
    (StatusCode::CREATED, "created").into_response()
}

async fn add_contract(
    AppState(state): AppState<State>,
    Json(req): Json<ContractRequest>,
) -> Response {
    state.add_contract(&req.community_id, req.contract).await;
    (StatusCode::CREATED, "created").into_response()
}

async fn put_manifest(
    AppState(state): AppState<State>,
    Json(manifest): Json<Manifest>,
) -> Response {
    state.put_manifest(manifest).await;
    (StatusCode::CREATED, "created").into_response()
}

async fn add_calendar(
    AppState(state): AppState<State>,
    Json(calendar): Json<Calendar>,
) -> Response {
    state.add_calendar(calendar).await;
    (StatusCode::CREATED, "created").into_response()
}

#[derive(Debug, serde::Deserialize)]
struct ContractRequest {
    community_id: String,
    contract: Contract,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct StateSnapshot {
    pub community: String,
    pub schedules: Vec<Schedule>,
    pub runs: Vec<Run>,
    pub contracts: Vec<Contract>,
    pub calendars: Vec<Calendar>,
}

async fn get_state(
    AppState(state): AppState<State>,
    Path(community): Path<String>,
) -> Json<StateSnapshot> {
    let schedules = state.schedules_for(&community).await;
    let runs: Vec<Run> = state.runs_for(&community).await.into_values().collect();
    let contracts = state.contracts_for(&community).await;
    let calendars = state.calendars_for(&community).await;
    Json(StateSnapshot {
        community,
        schedules,
        runs,
        contracts,
        calendars,
    })
}

async fn get_lineage(
    AppState(state): AppState<State>,
    Path((community, schedule_id)): Path<(String, String)>,
) -> Response {
    let contracts = state.contracts_for(&community).await;
    let mine: Vec<Contract> = contracts
        .iter()
        .filter(|c| c.schedule_id == schedule_id)
        .cloned()
        .collect();
    if mine.is_empty() {
        return (StatusCode::NOT_FOUND, "no contracts for schedule").into_response();
    }
    let runs = state.runs_for(&community).await;
    let Some(run) = runs.get(&schedule_id) else {
        return (StatusCode::NOT_FOUND, "no run for schedule").into_response();
    };
    let store = state.manifest_store_async().await;
    match check_inputs(&store, run, &mine).await {
        Ok(mut deps) => {
            // Enrich producer schedule ids for the API response, matching the
            // feed renderer's behavior.
            for d in deps.iter_mut() {
                d.producer_schedule_id = contracts
                    .iter()
                    .find(|c| {
                        c.role == almanac_bridge::model::ContractRole::Produce
                            && c.schema_id == d.schema_id
                    })
                    .map(|c| c.schedule_id.clone());
            }
            Json(deps).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("lineage error: {e}"),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::seed_demo;
    use axum::body::to_bytes;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn app() -> axum::Router {
        let state = State::new();
        seed_demo(&state).await;
        router(state)
    }

    #[tokio::test]
    async fn healthz_ok() {
        let app = app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn calendar_feed_returns_ics_with_events() {
        let app = app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/calendar/demo.ics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/calendar; charset=utf-8"
        );
        let body = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("BEGIN:VCALENDAR"));
        assert!(text.contains("UID:daily-brief@almanac"));
        assert!(text.contains("UID:weekly-strat"));
        // status overlay on succeeded run.
        let flat = text.replace("\n ", "").replace("\r\n ", "");
        assert!(flat.contains("SUMMARY:✅ Daily research brief"));
        assert!(flat.contains("STATUS:CONFIRMED"));
    }

    #[tokio::test]
    async fn runs_feed_only_includes_runs() {
        let app = app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/calendar/demo/runs.ics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("run-daily-001@almanac.run"));
    }

    #[tokio::test]
    async fn state_snapshot_json() {
        let app = app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/v1/communities/demo/state")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        let snap: StateSnapshot = serde_json::from_slice(&body).unwrap();
        assert!(!snap.schedules.is_empty());
        assert_eq!(snap.community, "demo");
    }

    #[tokio::test]
    async fn lineage_endpoint_returns_deps() {
        let app = app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/v1/communities/demo/lineage/weekly-strategy")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        let deps: Vec<Dependency> = serde_json::from_slice(&body).unwrap();
        assert_eq!(deps.len(), 1);
        // manifest is fresh and v3 >= min v2 → Ready
        assert!(matches!(
            deps[0].satisfied,
            almanac_bridge::lineage::Satisfies::Ready { .. }
        ));
    }

    #[tokio::test]
    async fn unknown_community_returns_empty_feed() {
        let app = app().await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/calendar/ghost.ics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("BEGIN:VCALENDAR"));
        // no events
        assert!(!text.contains("BEGIN:VEVENT"));
    }
}
