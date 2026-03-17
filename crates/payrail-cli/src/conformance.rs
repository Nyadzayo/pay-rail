use indicatif::{ProgressBar, ProgressStyle};
use payrail_output::config::{OutputConfig, Verbosity};
use payrail_output::{colors, format, symbols};
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Exit codes for conformance results.
#[allow(dead_code)]
pub const EXIT_ALL_PASS: i32 = 0;
pub const EXIT_FAILURES: i32 = 1;
#[allow(dead_code)]
pub const EXIT_VERIFY: i32 = 2;

#[derive(Debug)]
pub enum ConformanceError {
    AdapterNotFound(String),
    ScriptNotFound,
    NodeNotFound,
    MissingSandboxCredentials(String, String),
    PipelineFailed(String),
    Io(String),
}

impl std::fmt::Display for ConformanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdapterNotFound(provider) => write!(
                f,
                "[CONFORMANCE_NO_ADAPTER] No adapter found for '{provider}' [Run: payrail generate {provider}]"
            ),
            Self::ScriptNotFound => write!(
                f,
                "[CONFORMANCE_SCRIPT_MISSING] Conformance script not found [Run: pnpm --filter @payrail/mcp-server build]"
            ),
            Self::NodeNotFound => write!(
                f,
                "[CONFORMANCE_NO_NODE] Node.js not installed or not in PATH [Install Node.js v20+ from nodejs.org]"
            ),
            Self::MissingSandboxCredentials(provider, var) => write!(
                f,
                "[CONFORMANCE_NO_CREDS] Sandbox credentials not configured for '{provider}': {var} not set [Set env var: export {var}=<your-key>]"
            ),
            Self::PipelineFailed(msg) => write!(f, "[CONFORMANCE_FAILED] {msg}"),
            Self::Io(msg) => write!(f, "[CONFORMANCE_IO] {msg}"),
        }
    }
}

impl std::error::Error for ConformanceError {}

/// Format a ConformanceError for CLI output.
pub fn format_error(config: &OutputConfig, err: &ConformanceError) -> String {
    let (what, why, action) = match err {
        ConformanceError::AdapterNotFound(p) => (
            "No adapter found".to_owned(),
            format!("Adapter for '{p}' not found in project"),
            format!("Generate one first: payrail generate {p}"),
        ),
        ConformanceError::ScriptNotFound => (
            "Conformance script missing".to_owned(),
            "Script not found at expected path".to_owned(),
            "Run: pnpm --filter @payrail/mcp-server build".to_owned(),
        ),
        ConformanceError::NodeNotFound => (
            "Node.js not found".to_owned(),
            "Node.js is not installed or not in PATH".to_owned(),
            "Install Node.js v20+ from nodejs.org".to_owned(),
        ),
        ConformanceError::MissingSandboxCredentials(provider, var) => (
            "Sandbox credentials not configured".to_owned(),
            format!("Missing {var} for {provider}"),
            format!("Set env var: export {var}=<your-key>"),
        ),
        ConformanceError::PipelineFailed(msg) => (
            "Conformance test failed".to_owned(),
            msg.clone(),
            "Check adapter implementation and knowledge pack".to_owned(),
        ),
        ConformanceError::Io(msg) => (
            "I/O error".to_owned(),
            msg.clone(),
            "Check file paths and permissions".to_owned(),
        ),
    };

    if config.is_json() {
        let val = serde_json::json!({
            "status": "error",
            "error": what,
            "reason": why,
            "help": action,
        });
        serde_json::to_string(&val).unwrap()
    } else {
        format::error_message(config, &what, &why, &action)
    }
}

#[derive(Serialize)]
struct ConformanceReport {
    provider: String,
    action: String,
    sandbox: bool,
    output: String,
}

/// Derive the expected sandbox env var name from a provider name.
fn sandbox_env_var(provider: &str) -> String {
    let upper = provider.replace('-', "_").to_uppercase();
    format!("{upper}_SANDBOX_API_KEY")
}

/// Run conformance tests for a provider.
pub fn conformance_run(
    provider: &str,
    sandbox: bool,
    config: &OutputConfig,
) -> Result<i32, ConformanceError> {
    // Check sandbox credentials if needed
    if sandbox {
        let env_var = sandbox_env_var(provider);
        if std::env::var(&env_var).is_err() {
            return Err(ConformanceError::MissingSandboxCredentials(
                provider.to_owned(),
                env_var,
            ));
        }
    }

    // Check that a knowledge pack exists for this provider (proxy for adapter existence)
    let compiled_pack = Path::new("knowledge-packs")
        .join(provider)
        .join("compiled/pack.json");
    if !compiled_pack.exists() {
        return Err(ConformanceError::AdapterNotFound(provider.to_owned()));
    }

    // Locate the TypeScript conformance script.
    // This file is produced by `pnpm --filter @payrail/mcp-server build`.
    let script_path = Path::new("packages/mcp-server/dist/cli/conformance.js");
    if !script_path.exists() {
        return Err(ConformanceError::ScriptNotFound);
    }

    let spinner = if !config.is_json() {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message(format!("Running conformance tests for {provider}..."));
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    let mut cmd = Command::new("node");
    cmd.arg(script_path).arg(provider);

    if sandbox {
        cmd.arg("--sandbox");
    }

    if config.is_json() {
        cmd.arg("--format").arg("json");
    }

    if config.verbosity == Verbosity::Verbose {
        cmd.arg("--verbose");
    }

    let output = cmd.output().map_err(|e| {
        if let Some(ref s) = spinner {
            s.finish_and_clear();
        }
        if e.kind() == std::io::ErrorKind::NotFound {
            ConformanceError::NodeNotFound
        } else {
            ConformanceError::Io(format!("Failed to spawn conformance pipeline: {e}"))
        }
    })?;

    if let Some(ref s) = spinner {
        s.finish_and_clear();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Conformance tests may exit with non-zero for failures/verify items
    // Only treat as pipeline error if stderr indicates a crash
    let exit_code = output.status.code().unwrap_or(EXIT_FAILURES);

    if !output.status.success() && !stderr.is_empty() && stdout.is_empty() {
        // True pipeline failure (no test output, only errors)
        return Err(ConformanceError::PipelineFailed(stderr.trim().to_owned()));
    }

    if config.is_json() {
        let report = ConformanceReport {
            provider: provider.to_owned(),
            action: "conformance".into(),
            sandbox,
            output: stdout.trim().to_owned(),
        };
        println!("{}", serde_json::to_string(&report).unwrap());
    } else {
        // Parse exit code for summary styling
        let (sym, style_fn): (&str, fn(&OutputConfig, &str) -> String) = match exit_code {
            0 => (symbols::pass_symbol(config), colors::success),
            2 => (symbols::verify_symbol(config), colors::warning),
            _ => (symbols::fail_symbol(config), colors::error),
        };

        let summary = format!("{sym} {provider} Conformance");
        println!("{}", style_fn(config, &summary));

        if !stdout.is_empty() {
            println!();
            for line in stdout.trim().lines() {
                println!("{}", format::detail_line(line, 2));
            }
        }

        if exit_code == 0 {
            println!();
            println!(
                "{}",
                format::help_line(
                    config,
                    "Run with --verbose for detailed state transition results"
                )
            );
        }
    }

    Ok(exit_code)
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
    fn sandbox_env_var_derivation() {
        assert_eq!(
            sandbox_env_var("peach-payments"),
            "PEACH_PAYMENTS_SANDBOX_API_KEY"
        );
        assert_eq!(
            sandbox_env_var("startbutton"),
            "STARTBUTTON_SANDBOX_API_KEY"
        );
        assert_eq!(sandbox_env_var("stripe"), "STRIPE_SANDBOX_API_KEY");
    }

    #[test]
    #[serial]
    fn conformance_rejects_missing_sandbox_creds() {
        // Ensure the env var is not set
        let var = sandbox_env_var("nonexistent-provider");
        unsafe {
            std::env::remove_var(&var);
        }

        let result = conformance_run("nonexistent-provider", true, &test_config());
        assert!(result.is_err());
        match result.unwrap_err() {
            ConformanceError::MissingSandboxCredentials(p, v) => {
                assert_eq!(p, "nonexistent-provider");
                assert_eq!(v, "NONEXISTENT_PROVIDER_SANDBOX_API_KEY");
            }
            other => panic!("expected MissingSandboxCredentials, got: {other}"),
        }
    }

    #[test]
    fn conformance_rejects_missing_adapter() {
        // Without sandbox flag and no knowledge pack, hits AdapterNotFound
        let result = conformance_run("test-provider", false, &test_config());
        assert!(result.is_err());
        match result.unwrap_err() {
            ConformanceError::AdapterNotFound(p) => assert_eq!(p, "test-provider"),
            other => panic!("expected AdapterNotFound, got: {other}"),
        }
    }

    #[test]
    fn format_error_text_contains_parts() {
        let cfg = test_config();
        let err = ConformanceError::MissingSandboxCredentials(
            "peach".to_owned(),
            "PEACH_SANDBOX_API_KEY".to_owned(),
        );
        let formatted = format_error(&cfg, &err);
        assert!(formatted.contains("Sandbox credentials"));
        assert!(formatted.contains("PEACH_SANDBOX_API_KEY"));
    }

    #[test]
    fn format_error_json_is_valid() {
        let cfg = json_config();
        let err = ConformanceError::AdapterNotFound("test".to_owned());
        let formatted = format_error(&cfg, &err);
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["status"], "error");
    }

    #[test]
    fn exit_code_constants() {
        assert_eq!(EXIT_ALL_PASS, 0);
        assert_eq!(EXIT_FAILURES, 1);
        assert_eq!(EXIT_VERIFY, 2);
    }

    #[test]
    fn error_display_formats() {
        let err = ConformanceError::ScriptNotFound;
        assert!(err.to_string().contains("CONFORMANCE_SCRIPT_MISSING"));

        let err = ConformanceError::NodeNotFound;
        assert!(err.to_string().contains("Node.js"));

        let err = ConformanceError::AdapterNotFound("test".to_owned());
        assert!(err.to_string().contains("test"));
    }
}
