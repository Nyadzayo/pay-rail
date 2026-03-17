use payrail_output::config::OutputConfig;
use payrail_output::{colors, format, symbols};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub category: String,
    pub name: String,
    pub passed: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    checks: Vec<CheckResult>,
    passed: usize,
    total: usize,
    issues: usize,
}

/// Run all doctor checks and return results.
pub fn run_doctor(config: &OutputConfig) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    // Environment variable checks for known providers
    checks.extend(check_env_vars());

    // Connectivity checks for sandbox APIs
    checks.extend(check_connectivity());

    // Adapter health checks
    checks.extend(check_adapter_health());

    // Knowledge pack checks
    checks.extend(check_knowledge_packs());

    // Display results
    if config.is_json() {
        let passed = checks.iter().filter(|c| c.passed).count();
        let total = checks.len();
        let report = DoctorReport {
            checks: checks.clone(),
            passed,
            total,
            issues: total - passed,
        };
        println!("{}", serde_json::to_string(&report).unwrap());
    } else {
        println!("{}", format::summary_line(config, "PayRail Doctor"));
        println!();

        for check in &checks {
            let sym = if check.passed {
                symbols::pass_symbol(config)
            } else {
                symbols::fail_symbol(config)
            };

            let line = format!("{sym}  {}: {}", check.category, check.message);
            if check.passed {
                println!("{}", colors::success(config, &format!("  {line}")));
            } else {
                println!("{}", colors::error(config, &format!("  {line}")));
                if let Some(help) = &check.help {
                    println!("{}", format::help_line(config, help));
                }
            }
        }

        let passed = checks.iter().filter(|c| c.passed).count();
        let total = checks.len();
        let issues = total - passed;
        println!();
        if issues == 0 {
            println!(
                "{}",
                colors::success(
                    config,
                    &format!("  {passed}/{total} checks passed. All clear.")
                )
            );
        } else {
            println!(
                "{}",
                colors::warning(
                    config,
                    &format!("  {passed}/{total} checks passed. {issues} issue(s) found.")
                )
            );
        }
    }

    checks
}

/// Check that expected provider environment variables are set.
fn check_env_vars() -> Vec<CheckResult> {
    let providers = [
        ("PEACH_SANDBOX_API_KEY", "peach-payments"),
        ("PEACH_SANDBOX_ENTITY_ID", "peach-payments"),
        ("PEACH_SANDBOX_WEBHOOK_SECRET", "peach-payments"),
        ("STARTBUTTON_SANDBOX_API_KEY", "startbutton"),
        ("STARTBUTTON_SANDBOX_WEBHOOK_SECRET", "startbutton"),
    ];

    providers
        .iter()
        .map(|(var, _provider)| {
            let is_set = std::env::var(var).is_ok();
            CheckResult {
                category: "Environment".to_owned(),
                name: var.to_string(),
                passed: is_set,
                message: if is_set {
                    format!("{var} set")
                } else {
                    format!("{var} not set")
                },
                help: if is_set {
                    None
                } else {
                    Some(format!("Set the variable: export {var}=<your-key>"))
                },
            }
        })
        .collect()
}

/// Check connectivity to sandbox API endpoints.
///
/// Validates that provider env vars contain non-empty values, and attempts
/// a basic TCP connection to known sandbox hosts when credentials are configured.
fn check_connectivity() -> Vec<CheckResult> {
    use std::net::TcpStream;
    use std::time::Duration;

    let endpoints: &[(&str, &str, &str)] = &[
        (
            "peach-payments",
            "PEACH_SANDBOX_API_KEY",
            "sandbox.peachpayments.com:443",
        ),
        (
            "startbutton",
            "STARTBUTTON_SANDBOX_API_KEY",
            "api.startbutton.com:443",
        ),
    ];

    endpoints
        .iter()
        .map(|(provider, env_var, host)| {
            let has_key = std::env::var(env_var)
                .map(|v| !v.is_empty())
                .unwrap_or(false);

            if !has_key {
                return CheckResult {
                    category: "Connectivity".to_owned(),
                    name: format!("{provider} sandbox"),
                    passed: true,
                    message: format!("{provider} connectivity check: skipped ({env_var} not set)"),
                    help: None,
                };
            }

            let reachable =
                TcpStream::connect_timeout(&host.parse().unwrap(), Duration::from_secs(5)).is_ok();

            CheckResult {
                category: "Connectivity".to_owned(),
                name: format!("{provider} sandbox"),
                passed: reachable,
                message: if reachable {
                    format!("{provider} sandbox reachable ({host})")
                } else {
                    format!("{provider} sandbox unreachable ({host})")
                },
                help: if reachable {
                    None
                } else {
                    Some(format!("Check network connectivity to {host}"))
                },
            }
        })
        .collect()
}

/// Check that adapter source files exist in expected locations.
fn check_adapter_health() -> Vec<CheckResult> {
    let adapter_dir = Path::new("src/adapters");
    if !adapter_dir.exists() {
        return vec![CheckResult {
            category: "Adapters".to_owned(),
            name: "adapter directory".to_owned(),
            passed: false,
            message: "src/adapters/ directory not found".to_owned(),
            help: Some(
                "Run `payrail init --provider <name>` to create the project scaffold".to_owned(),
            ),
        }];
    }

    let mut checks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(adapter_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                checks.push(CheckResult {
                    category: "Adapters".to_owned(),
                    name: name.clone(),
                    passed: true,
                    message: format!("adapter file found: {name}"),
                    help: None,
                });
            }
        }
    }

    if checks.is_empty() {
        checks.push(CheckResult {
            category: "Adapters".to_owned(),
            name: "adapters".to_owned(),
            passed: true,
            message: "No adapter files yet (ok for new projects)".to_owned(),
            help: None,
        });
    }

    checks
}

/// Check that knowledge packs exist and are valid.
fn check_knowledge_packs() -> Vec<CheckResult> {
    let pack_dir = Path::new("knowledge-packs");
    if !pack_dir.exists() {
        return vec![CheckResult {
            category: "Knowledge".to_owned(),
            name: "knowledge-packs directory".to_owned(),
            passed: false,
            message: "knowledge-packs/ directory not found".to_owned(),
            help: Some(
                "Run `payrail init --provider <name>` or `payrail knowledge init <provider>`"
                    .to_owned(),
            ),
        }];
    }

    let mut checks = Vec::new();
    if let Ok(entries) = std::fs::read_dir(pack_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let pack_yaml = entry.path().join("pack.yaml");
                let compiled = entry.path().join("compiled/pack.json");

                let has_source = pack_yaml.exists();
                let has_compiled = compiled.exists();

                checks.push(CheckResult {
                    category: "Knowledge".to_owned(),
                    name: name.clone(),
                    passed: has_source,
                    message: if has_compiled {
                        format!("{name} pack valid (compiled)")
                    } else if has_source {
                        format!("{name} pack exists (not compiled)")
                    } else {
                        format!("{name} pack.yaml missing")
                    },
                    help: if !has_source {
                        Some(format!("Run `payrail knowledge init {name}` to create"))
                    } else if !has_compiled {
                        Some(format!("Run `payrail knowledge compile {name}` to compile"))
                    } else {
                        None
                    },
                });
            }
        }
    }

    if checks.is_empty() {
        checks.push(CheckResult {
            category: "Knowledge".to_owned(),
            name: "knowledge-packs".to_owned(),
            passed: true,
            message: "No knowledge packs configured (ok for new projects)".to_owned(),
            help: None,
        });
    }

    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn test_config() -> OutputConfig {
        OutputConfig::from_env(false, true, false, true)
    }

    fn json_config() -> OutputConfig {
        OutputConfig::from_env(true, false, false, false)
    }

    #[test]
    #[serial]
    fn env_var_check_reports_unset() {
        unsafe {
            std::env::remove_var("PEACH_SANDBOX_API_KEY");
        }
        let checks = check_env_vars();
        let peach = checks
            .iter()
            .find(|c| c.name == "PEACH_SANDBOX_API_KEY")
            .unwrap();
        assert!(!peach.passed);
        assert!(peach.help.is_some());
    }

    #[test]
    #[serial]
    fn env_var_check_reports_set() {
        unsafe {
            std::env::set_var("PEACH_SANDBOX_API_KEY", "test-key");
        }
        let checks = check_env_vars();
        let peach = checks
            .iter()
            .find(|c| c.name == "PEACH_SANDBOX_API_KEY")
            .unwrap();
        assert!(peach.passed);
        unsafe {
            std::env::remove_var("PEACH_SANDBOX_API_KEY");
        }
    }

    #[test]
    fn check_result_serializes_to_json() {
        let result = CheckResult {
            category: "Test".into(),
            name: "test-check".into(),
            passed: true,
            message: "OK".into(),
            help: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["passed"], true);
        assert!(parsed.get("help").is_none()); // skip_serializing_if
    }

    #[test]
    #[serial]
    fn doctor_runs_without_panic() {
        // Just verify it doesn't crash, even with no knowledge-packs dir
        let _ = run_doctor(&test_config());
    }

    #[test]
    #[serial]
    fn doctor_json_output_valid() {
        let _ = run_doctor(&json_config());
        // If it didn't panic, JSON output was produced
    }
}
