//! # almanac-server
//!
//! Standalone HTTP server that renders Almanac iCalendar feeds from an
//! in-memory state store. In a Buzz deployment the same routes attach to
//! the relay's router and read from the relay's event store; this crate
//! exists so Almanac is independently runnable, testable, and demonstrable.
//!
//! ## Quick start
//!
//! ```no_run
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! almanac_server::run(almanac_server::Config::default()).await?;
//! # Ok(()) }
//! ```
//!
//! Then subscribe to `http://localhost:8787/calendar/demo.ics` in any
//! calendar app.

#![forbid(unsafe_code)]

pub mod http;
pub mod seed;
pub mod store;

pub use http::{router, StateSnapshot};
pub use store::State;

pub use almanac_bridge::Config;

use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Run the Almanac server to completion.
///
/// Seeds the demo community if `config.seed_demo` is true.
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = State::with_default_community(config.default_community.clone());
    if config.seed_demo {
        seed::seed_demo(&state).await;
        info!(community = %config.default_community, "seeded demo community");
    }
    serve(state, &config).await
}

/// Serve the router on the configured bind address. Used by both `run` and tests.
pub async fn serve(
    state: State,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = router(state).layer(TraceLayer::new_for_http());
    let addr: SocketAddr = config.bind.parse()?;
    info!(%addr, "almanac server listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
