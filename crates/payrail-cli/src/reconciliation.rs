use chrono::{Duration, Utc};
use payrail_core::reconciliation::{DiscrepancyBreakdown, ReconciliationReport, ResolutionSummary};
use payrail_output::config::OutputConfig;
use payrail_output::{colors, format, symbols};
use serde::Serialize;

/// CLI-facing reconciliation report for JSON serialization.
#[derive(Debug, Clone, Serialize)]
pub struct CliReconciliationReport {
    pub provider: String,
    pub period: CliPeriod,
    pub total_payments: u64,
    pub matched: u64,
    pub match_rate: f64,
    pub discrepancies: CliDiscrepancies,
    pub settlements: u64,
    pub resolutions: CliResolutions,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliPeriod {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliDiscrepancies {
    pub timing_delay: u64,
    pub material_mismatch: u64,
    pub permanent_divergence: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliResolutions {
    pub auto_resolved: u64,
    pub escalated: u64,
    pub manually_resolved: u64,
}

impl From<&ReconciliationReport> for CliReconciliationReport {
    fn from(r: &ReconciliationReport) -> Self {
        Self {
            provider: r.provider.clone(),
            period: CliPeriod {
                start: r.period_start.to_rfc3339(),
                end: r.period_end.to_rfc3339(),
            },
            total_payments: r.total_payments,
            matched: r.matched_count,
            match_rate: (r.match_rate * 10.0).round() / 10.0,
            discrepancies: CliDiscrepancies {
                timing_delay: r.discrepancies.timing_delay_count,
                material_mismatch: r.discrepancies.material_mismatch_count,
                permanent_divergence: r.discrepancies.permanent_divergence_count,
            },
            settlements: r.settlements,
            resolutions: CliResolutions {
                auto_resolved: r.resolutions.auto_resolved_count,
                escalated: r.resolutions.escalated_count,
                manually_resolved: r.resolutions.manually_resolved_count,
            },
        }
    }
}

/// Parse a period string like "1h", "12h", "24h", "7d" into a duration.
pub fn parse_period(period: &str) -> Duration {
    match period {
        "1h" => Duration::hours(1),
        "12h" => Duration::hours(12),
        "24h" => Duration::hours(24),
        "7d" => Duration::days(7),
        _ => Duration::hours(24),
    }
}

/// Generate a placeholder reconciliation report for a provider.
/// In production, this would query the event store. For now, returns zeros.
pub fn placeholder_report(provider: &str, period: &str) -> ReconciliationReport {
    let now = Utc::now();
    let duration = parse_period(period);
    let start = now - duration;

    ReconciliationReport {
        provider: provider.to_string(),
        period_start: start,
        period_end: now,
        total_payments: 0,
        matched_count: 0,
        match_rate: 100.0,
        discrepancies: DiscrepancyBreakdown::default(),
        resolutions: ResolutionSummary::default(),
        settlements: 0,
    }
}

/// Show reconciliation status for a provider.
pub fn show_reconciliation(provider: Option<&str>, period: &str, config: &OutputConfig) {
    let providers = match provider {
        Some(p) => vec![p.to_string()],
        None => vec!["(all providers)".to_string()],
    };

    for provider_name in &providers {
        let report = placeholder_report(provider_name, period);

        if config.is_json() {
            let cli_report = CliReconciliationReport::from(&report);
            println!("{}", serde_json::to_string(&cli_report).unwrap());
            return;
        }

        print_human_readable(&report, period, config);
    }
}

fn print_human_readable(report: &ReconciliationReport, period: &str, config: &OutputConfig) {
    println!(
        "{}",
        format::summary_line(
            config,
            &format!(
                "Reconciliation Report  {}  Last {}",
                report.provider, period
            )
        )
    );
    println!();

    println!(
        "{}",
        format::detail_line(&format!("Total payments: {}", report.total_payments), 1)
    );

    let rate_str = format!(
        "Match rate: {:.1}% ({}/{})",
        report.match_rate, report.matched_count, report.total_payments
    );
    if report.match_rate >= 99.0 {
        println!(
            "{}",
            format::detail_line(&colors::success(config, &rate_str), 1,)
        );
    } else if report.match_rate >= 95.0 {
        println!(
            "{}",
            format::detail_line(&colors::warning(config, &rate_str), 1,)
        );
    } else {
        println!(
            "{}",
            format::detail_line(&colors::error(config, &rate_str), 1,)
        );
    }

    println!();

    let total_discrepancies = report.discrepancies.timing_delay_count
        + report.discrepancies.material_mismatch_count
        + report.discrepancies.permanent_divergence_count;

    if total_discrepancies > 0 {
        println!("{}", format::detail_line("Discrepancies:", 1));
        println!(
            "{}",
            format::detail_line(
                &format!(
                    "Timing delays:         {} (auto-resolved)",
                    report.discrepancies.timing_delay_count
                ),
                2,
            )
        );
        println!(
            "{}",
            format::detail_line(
                &format!(
                    "Material mismatches:   {} (escalated)",
                    report.discrepancies.material_mismatch_count
                ),
                2,
            )
        );
        println!(
            "{}",
            format::detail_line(
                &format!(
                    "Permanent divergence:  {} (high priority)",
                    report.discrepancies.permanent_divergence_count
                ),
                2,
            )
        );
    } else {
        println!(
            "{}",
            format::detail_line(
                &colors::success(
                    config,
                    &format!("{} No discrepancies", symbols::pass_symbol(config)),
                ),
                1,
            )
        );
    }

    println!();
    println!(
        "{}",
        format::detail_line(
            &format!(
                "Settlements: {} payments transitioned to Settled",
                report.settlements
            ),
            1,
        )
    );
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
    fn parse_period_1h() {
        assert_eq!(parse_period("1h"), Duration::hours(1));
    }

    #[test]
    fn parse_period_12h() {
        assert_eq!(parse_period("12h"), Duration::hours(12));
    }

    #[test]
    fn parse_period_24h() {
        assert_eq!(parse_period("24h"), Duration::hours(24));
    }

    #[test]
    fn parse_period_7d() {
        assert_eq!(parse_period("7d"), Duration::days(7));
    }

    #[test]
    fn parse_period_unknown_defaults_to_24h() {
        assert_eq!(parse_period("bogus"), Duration::hours(24));
    }

    #[test]
    fn placeholder_report_has_correct_provider() {
        let report = placeholder_report("peach_payments", "24h");
        assert_eq!(report.provider, "peach_payments");
        assert_eq!(report.total_payments, 0);
        assert!((report.match_rate - 100.0).abs() < 0.01);
    }

    #[test]
    fn placeholder_report_period_bounds() {
        let before = Utc::now();
        let report = placeholder_report("test", "1h");
        let after = Utc::now();

        assert!(report.period_start >= before - Duration::hours(1) - Duration::seconds(1));
        assert!(report.period_end <= after + Duration::seconds(1));
    }

    #[test]
    fn cli_report_from_core_report() {
        let report = placeholder_report("peach_payments", "24h");
        let cli_report = CliReconciliationReport::from(&report);
        assert_eq!(cli_report.provider, "peach_payments");
        assert_eq!(cli_report.total_payments, 0);
        assert_eq!(cli_report.matched, 0);
        assert!((cli_report.match_rate - 100.0).abs() < 0.01);
        assert_eq!(cli_report.settlements, 0);
    }

    #[test]
    fn cli_report_serializes_to_json() {
        let report = placeholder_report("peach_payments", "24h");
        let cli_report = CliReconciliationReport::from(&report);
        let json = serde_json::to_string(&cli_report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["provider"], "peach_payments");
        assert_eq!(parsed["total_payments"], 0);
        assert!(parsed["period"]["start"].is_string());
        assert!(parsed["period"]["end"].is_string());
        assert_eq!(parsed["discrepancies"]["timing_delay"], 0);
        assert_eq!(parsed["resolutions"]["auto_resolved"], 0);
    }

    #[test]
    fn show_reconciliation_text_runs_without_panic() {
        show_reconciliation(Some("test_provider"), "24h", &test_config());
    }

    #[test]
    fn show_reconciliation_json_runs_without_panic() {
        show_reconciliation(Some("test_provider"), "7d", &json_config());
    }

    #[test]
    fn show_reconciliation_no_provider_runs_without_panic() {
        show_reconciliation(None, "24h", &test_config());
    }
}
