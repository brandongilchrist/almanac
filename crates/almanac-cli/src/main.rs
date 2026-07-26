//! `almanac` — the Almanac CLI.
//!
//! Subcommands:
//! - `subscribe [--community <id>]` — print the calendar subscribe URL.
//! - `check <schedule-id> [--community <id>]` — print lineage state for a schedule.
//! - `declare --schedule <id> --role produce|consume --schema <id> [...]` — write a contract.
//! - `serve` — run the standalone HTTP server (with demo data).
//! - `demo` — render the demo feed to stdout without a server.
//! - `validate <file.ics>` — validate an ICS file parses.

use almanac_bridge::model::{Contract, ContractRole};
use almanac_bridge::Config;
use clap::{Parser, Subcommand};
use icalendar::Calendar;
use std::process::ExitCode;
use std::str::FromStr;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "almanac",
    version,
    about = "A calendar for agents and their artifacts.",
    long_about = "Almanac renders scheduled agent work as standard iCalendar feeds,\n\
                  with artifact-lineage checkmarks (✅/❌/⚠️)."
)]
struct Cli {
    /// Base URL of an Almanac server (overrides ALMANAC_PUBLIC_URL).
    #[arg(long, env = "ALMANAC_URL", global = true)]
    url: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the calendar subscribe URL for a community.
    Subscribe {
        #[arg(long, env = "ALMANAC_COMMUNITY", default_value = "demo")]
        community: String,
        /// Print the runs-only feed URL instead.
        #[arg(long)]
        runs: bool,
    },
    /// Query a schedule's input lineage (✅/❌/⚠️ per input).
    Check {
        schedule_id: String,
        #[arg(long, env = "ALMANAC_COMMUNITY", default_value = "demo")]
        community: String,
    },
    /// Emit a KIND_ALMANAC_CONTRACT event body (JSON to stdout).
    Declare(DeclareArgs),
    /// Run the standalone HTTP server with demo data.
    Serve,
    /// Render the demo feed to stdout (no server needed).
    Demo,
    /// Validate that an .ics file parses as iCalendar.
    Validate { path: String },
}

#[derive(Debug, clap::Args)]
struct DeclareArgs {
    #[arg(long)]
    schedule: String,
    #[arg(long, value_name = "produce|consume")]
    role: RoleArg,
    #[arg(long)]
    schema: String,
    /// For consume: minimum producer version (>= 1). Default 1.
    #[arg(long, default_value_t = 1)]
    min_version: u32,
    /// For consume: accept any version (skip version check).
    #[arg(long)]
    any_version: bool,
    /// Freshness window in seconds (default 86400 = 24h).
    #[arg(long, default_value_t = Contract::DEFAULT_FRESHNESS_WINDOW)]
    freshness: u64,
    /// Community id the schedule lives in.
    #[arg(long, env = "ALMANAC_COMMUNITY", default_value = "demo")]
    community: String,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum RoleArg {
    Produce,
    Consume,
}

fn config_from_cli(cli: &Cli) -> Config {
    let mut c = Config::from_env();
    if let Some(url) = &cli.url {
        c.public_base_url = url.clone();
    }
    c
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("almanac: error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), anyhow::Error> {
    let config = config_from_cli(&cli);
    match cli.command {
        Command::Subscribe { community, runs } => {
            let url = if runs {
                config.runs_url(&community)
            } else {
                config.calendar_url(&community)
            };
            println!("{url}");
        }
        Command::Check {
            schedule_id,
            community,
        } => {
            check(&config, &community, &schedule_id).await?;
        }
        Command::Declare(args) => {
            declare(args)?;
        }
        Command::Serve => {
            almanac_server::run(config)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        }
        Command::Demo => {
            demo().await?;
        }
        Command::Validate { path } => {
            validate(&path)?;
        }
    }
    Ok(())
}

async fn check(config: &Config, community: &str, schedule_id: &str) -> Result<(), anyhow::Error> {
    let url = format!(
        "{}/v1/communities/{}/lineage/{}",
        config.public_base_url.trim_end_matches('/'),
        community,
        schedule_id
    );
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("server returned {}: {}", resp.status(), resp.text().await?);
    }
    let deps: Vec<almanac_bridge::lineage::Dependency> = resp.json().await?;
    if deps.is_empty() {
        println!("Schedule `{schedule_id}` has no consuming contracts.");
        return Ok(());
    }
    println!("Lineage for `{schedule_id}` ({community}):");
    for d in &deps {
        let mark = match &d.satisfied {
            almanac_bridge::lineage::Satisfies::Ready { version, .. } => {
                format!("✅ ready (v{version})")
            }
            almanac_bridge::lineage::Satisfies::Missing => "❌ missing".to_string(),
            almanac_bridge::lineage::Satisfies::VersionMismatch { found, need } => {
                format!("⚠️ version mismatch (found v{found}, need v{need}+)")
            }
        };
        let producer = d
            .producer_schedule_id
            .as_deref()
            .unwrap_or("<unknown producer>");
        println!("  {mark}  {}  ← produced by `{}`", d.schema_id, producer);
    }
    Ok(())
}

fn declare(args: DeclareArgs) -> Result<(), anyhow::Error> {
    let contract = Contract {
        contract_id: format!("{}-{}-{}", args.schedule, args.role_as_str(), args.schema),
        schedule_id: args.schedule.clone(),
        role: match args.role {
            RoleArg::Produce => ContractRole::Produce,
            RoleArg::Consume => ContractRole::Consume,
        },
        schema_id: args.schema.clone(),
        min_version: args.min_version,
        any_version: args.any_version,
        freshness_window: args.freshness,
    };
    let event = contract_to_event(&contract, &args.community);
    let json = serde_json::to_string_pretty(&event)?;
    println!("{json}");
    Ok(())
}

impl DeclareArgs {
    fn role_as_str(&self) -> &'static str {
        match self.role {
            RoleArg::Produce => "produce",
            RoleArg::Consume => "consume",
        }
    }
}

/// Build a NIP-33-shaped event envelope for a contract.
fn contract_to_event(contract: &Contract, community: &str) -> serde_json::Value {
    use almanac_bridge::kinds::KIND_ALMANAC_CONTRACT;
    serde_json::json!({
        "kind": KIND_ALMANAC_CONTRACT,
        "content": "",
        "tags": [
            ["d", contract.contract_id],
            ["schedule", contract.schedule_id],
            ["community", community],
            ["contract_role", match contract.role {
                ContractRole::Produce => "produce",
                ContractRole::Consume => "consume",
            }],
            ["schema_id", contract.schema_id],
            ["min_version", contract.min_version.to_string()],
            ["any_version", contract.any_version.to_string()],
            ["freshness_window", contract.freshness_window.to_string()],
        ]
    })
}

async fn demo() -> Result<(), anyhow::Error> {
    use almanac_bridge::ical::{render_calendar_feed, FeedFilter};
    use almanac_server::seed::seed_demo;
    use almanac_server::State;
    use std::collections::HashMap;

    let state = State::new();
    seed_demo(&state).await;
    let schedules = state.schedules_for("demo").await;
    let runs = state.runs_for("demo").await;
    let contracts = state.contracts_for("demo").await;
    let calendars = state.calendars_for("demo").await;

    let manifest_store = state.manifest_store_async().await;
    let mut deps_map = HashMap::new();
    for sched in &schedules {
        let mine: Vec<Contract> = contracts
            .iter()
            .filter(|c| c.schedule_id == sched.schedule_id)
            .cloned()
            .collect();
        if let Some(run) = runs.get(&sched.schedule_id) {
            if let Ok(deps) =
                almanac_bridge::lineage::check_inputs(&manifest_store, run, &mine).await
            {
                deps_map.insert(sched.schedule_id.clone(), deps);
            }
        }
    }

    let feed = render_calendar_feed(
        &schedules,
        &runs,
        &deps_map,
        &calendars,
        FeedFilter::Schedule,
        "demo",
    )?;
    print!("{feed}");
    Ok(())
}

fn validate(path: &str) -> Result<(), anyhow::Error> {
    let content = std::fs::read_to_string(path)?;
    match Calendar::from_str(&content) {
        Ok(cal) => {
            let n = cal.iter().count();
            println!("✅ valid iCalendar — {n} component(s)");
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("invalid iCalendar: {e}");
        }
    }
}
