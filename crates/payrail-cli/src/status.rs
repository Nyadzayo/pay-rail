use payrail_output::config::OutputConfig;
use payrail_output::{colors, format, symbols};
use serde::Serialize;

/// Operational status report — extensible for Epic 10 SCRAM data.
#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub period: String,
    pub webhooks_received: u64,
    pub duplicates_caught: u64,
    pub state_transitions: u64,
    pub signature_failures: u64,
    pub action_required: u64,
}

impl StatusReport {
    /// Create a placeholder report (event store not yet wired).
    pub fn placeholder(period: &str) -> Self {
        Self {
            period: period.to_owned(),
            webhooks_received: 0,
            duplicates_caught: 0,
            state_transitions: 0,
            signature_failures: 0,
            action_required: 0,
        }
    }
}

/// Display operational status.
pub fn show_status(period: &str, config: &OutputConfig) {
    // TODO: Query event store when available. For now, show placeholder.
    let report = StatusReport::placeholder(period);

    if config.is_json() {
        println!("{}", serde_json::to_string(&report).unwrap());
        return;
    }

    println!(
        "{}",
        format::summary_line(config, &format!("PayRail Status  Last {}", report.period))
    );
    println!();
    println!(
        "{}",
        format::detail_line(
            &format!("Webhooks received: {}", report.webhooks_received),
            1
        )
    );
    println!(
        "{}",
        format::detail_line(
            &format!("Duplicates caught: {}", report.duplicates_caught),
            1
        )
    );
    println!(
        "{}",
        format::detail_line(
            &format!("State transitions: {}", report.state_transitions),
            1
        )
    );
    println!(
        "{}",
        format::detail_line(
            &format!("Signature failures: {}", report.signature_failures),
            1
        )
    );
    println!();

    if report.action_required == 0 {
        println!(
            "{}",
            colors::success(
                config,
                &format!("  {} Action required: 0", symbols::pass_symbol(config))
            )
        );
    } else {
        println!(
            "{}",
            colors::warning(
                config,
                &format!(
                    "  {} {} items for review",
                    symbols::verify_symbol(config),
                    report.action_required
                )
            )
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> OutputConfig {
        OutputConfig::from_env(false, true, false, true)
    }

    fn json_config() -> OutputConfig {
        OutputConfig::from_env(true, false, false, false)
    }

    #[test]
    fn placeholder_report_is_all_zeros() {
        let report = StatusReport::placeholder("24h");
        assert_eq!(report.webhooks_received, 0);
        assert_eq!(report.duplicates_caught, 0);
        assert_eq!(report.state_transitions, 0);
        assert_eq!(report.signature_failures, 0);
        assert_eq!(report.action_required, 0);
        assert_eq!(report.period, "24h");
    }

    #[test]
    fn status_report_serializes_to_json() {
        let report = StatusReport::placeholder("1h");
        let json = serde_json::to_string(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["period"], "1h");
        assert_eq!(parsed["webhooks_received"], 0);
    }

    #[test]
    fn show_status_runs_without_panic() {
        show_status("24h", &test_config());
    }

    #[test]
    fn show_status_json_runs_without_panic() {
        show_status("7d", &json_config());
    }
}
