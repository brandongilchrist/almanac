//! Configuration — env vars and feature flags.

use std::env;

/// Almanac runtime configuration, loaded from environment.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP bind address (e.g. "0.0.0.0:8787").
    pub bind: String,
    /// Default community id used by the demo / standalone server.
    pub default_community: String,
    /// Whether to seed the demo dataset on startup.
    pub seed_demo: bool,
    /// Base URL for constructing subscribe URLs (e.g. "http://localhost:8787").
    pub public_base_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8787".into(),
            default_community: "demo".into(),
            seed_demo: true,
            public_base_url: "http://localhost:8787".into(),
        }
    }
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Recognized vars:
    /// - `ALMANAC_BIND` — bind address (default `0.0.0.0:8787`)
    /// - `ALMANAC_COMMUNITY` — default community id (default `demo`)
    /// - `ALMANAC_SEED_DEMO` — `1`/`true` to seed demo data (default `true`)
    /// - `ALMANAC_PUBLIC_URL` — public base URL (default `http://localhost:8787`)
    pub fn from_env() -> Self {
        let mut c = Self::default();
        if let Ok(v) = env::var("ALMANAC_BIND") {
            c.bind = v;
        }
        if let Ok(v) = env::var("ALMANAC_COMMUNITY") {
            c.default_community = v;
        }
        if let Ok(v) = env::var("ALMANAC_SEED_DEMO") {
            c.seed_demo = matches!(v.as_str(), "1" | "true" | "TRUE" | "yes");
        }
        if let Ok(v) = env::var("ALMANAC_PUBLIC_URL") {
            c.public_base_url = v;
        }
        c
    }

    /// Construct the subscribe URL for a community.
    pub fn calendar_url(&self, community: &str) -> String {
        format!(
            "{}/calendar/{}.ics",
            self.public_base_url.trim_end_matches('/'),
            community
        )
    }

    /// Construct the runs-only subscribe URL.
    pub fn runs_url(&self, community: &str) -> String {
        format!(
            "{}/calendar/{}/runs.ics",
            self.public_base_url.trim_end_matches('/'),
            community
        )
    }

    /// Construct the schedule-only subscribe URL.
    pub fn schedule_url(&self, community: &str) -> String {
        format!(
            "{}/calendar/{}/schedule.ics",
            self.public_base_url.trim_end_matches('/'),
            community
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_construction() {
        let c = Config {
            public_base_url: "http://example.com".into(),
            ..Default::default()
        };
        assert_eq!(
            c.calendar_url("research"),
            "http://example.com/calendar/research.ics"
        );
        assert_eq!(
            c.runs_url("research"),
            "http://example.com/calendar/research/runs.ics"
        );
        assert_eq!(
            c.schedule_url("research"),
            "http://example.com/calendar/research/schedule.ics"
        );
    }

    #[test]
    fn url_construction_strips_trailing_slash() {
        let c = Config {
            public_base_url: "http://example.com/".into(),
            ..Default::default()
        };
        assert_eq!(c.calendar_url("x"), "http://example.com/calendar/x.ics");
    }
}
