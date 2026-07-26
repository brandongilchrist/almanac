//! almanac-server binary entrypoint.

use almanac_server::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();
    eprintln!("Almanac server starting on http://{}", config.bind);
    eprintln!(
        "  Subscribe:  {}",
        config.calendar_url(&config.default_community)
    );
    eprintln!(
        "  Runs feed:  {}",
        config.runs_url(&config.default_community)
    );
    eprintln!(
        "  JSON state: http://{}/v1/communities/{}/state",
        config.bind, config.default_community
    );
    if config.seed_demo {
        eprintln!(
            "  Demo data seeded for community `{}`.",
            config.default_community
        );
    }

    almanac_server::run(config).await
}
