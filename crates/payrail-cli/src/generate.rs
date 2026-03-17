use indicatif::{ProgressBar, ProgressStyle};
use payrail_output::config::OutputConfig;
use payrail_output::{format, symbols};
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[derive(Debug)]
pub enum GenerateError {
    KnowledgePackNotFound(String),
    ScriptNotFound,
    NodeNotFound,
    PipelineFailed(String),
    Io(String),
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KnowledgePackNotFound(provider) => write!(
                f,
                "[GENERATE_NO_PACK] No knowledge pack for '{provider}' [Run: payrail knowledge init {provider}]"
            ),
            Self::ScriptNotFound => write!(
                f,
                "[GENERATE_SCRIPT_MISSING] Generation script not found [Run: pnpm --filter @payrail/mcp-server build]"
            ),
            Self::NodeNotFound => write!(
                f,
                "[GENERATE_NO_NODE] Node.js not installed or not in PATH [Install Node.js v20+ from nodejs.org]"
            ),
            Self::PipelineFailed(msg) => write!(f, "[GENERATE_FAILED] {msg}"),
            Self::Io(msg) => write!(f, "[GENERATE_IO] {msg}"),
        }
    }
}

impl std::error::Error for GenerateError {}

/// Format a GenerateError for CLI output.
pub fn format_error(config: &OutputConfig, err: &GenerateError) -> String {
    let (what, why, action) = match err {
        GenerateError::KnowledgePackNotFound(p) => (
            "No knowledge pack found".to_owned(),
            format!("Knowledge pack for '{p}' not found"),
            format!("Create one first: payrail knowledge init {p}"),
        ),
        GenerateError::ScriptNotFound => (
            "Generation script missing".to_owned(),
            "Script not found at expected path".to_owned(),
            "Run: pnpm --filter @payrail/mcp-server build".to_owned(),
        ),
        GenerateError::NodeNotFound => (
            "Node.js not found".to_owned(),
            "Node.js is not installed or not in PATH".to_owned(),
            "Install Node.js v20+ from nodejs.org".to_owned(),
        ),
        GenerateError::PipelineFailed(msg) => (
            "Generation failed".to_owned(),
            msg.clone(),
            "Check knowledge pack and project configuration".to_owned(),
        ),
        GenerateError::Io(msg) => (
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
struct GenerationReport {
    provider: String,
    action: String,
    output: String,
}

/// Generate an adapter for a provider using codebase fingerprinting.
///
/// Returns exit code: 0 on success, 2 if VERIFY items are present in output.
pub fn generate_adapter(provider: &str, config: &OutputConfig) -> Result<i32, GenerateError> {
    // Check for knowledge pack
    let pack_dir = Path::new("knowledge-packs").join(provider);
    let compiled_pack = pack_dir.join("compiled/pack.json");
    let pack_yaml = pack_dir.join("pack.yaml");

    if !compiled_pack.exists() && !pack_yaml.exists() {
        return Err(GenerateError::KnowledgePackNotFound(provider.to_owned()));
    }

    // Locate the TypeScript generation script.
    // This file is produced by `pnpm --filter @payrail/mcp-server build`.
    let script_path = Path::new("packages/mcp-server/dist/cli/generate.js");
    if !script_path.exists() {
        return Err(GenerateError::ScriptNotFound);
    }

    let spinner = if !config.is_json() {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message(format!("Generating adapter for {provider}..."));
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    let mut cmd = Command::new("node");
    cmd.arg(script_path).arg(provider);

    if config.is_json() {
        cmd.arg("--format").arg("json");
    }

    let output = cmd.output().map_err(|e| {
        if let Some(ref s) = spinner {
            s.finish_and_clear();
        }
        if e.kind() == std::io::ErrorKind::NotFound {
            GenerateError::NodeNotFound
        } else {
            GenerateError::Io(format!("Failed to spawn generation pipeline: {e}"))
        }
    })?;

    if let Some(ref s) = spinner {
        s.finish_and_clear();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        let detail = if stderr.is_empty() {
            "unknown error".to_owned()
        } else {
            stderr.trim().to_owned()
        };
        return Err(GenerateError::PipelineFailed(detail));
    }

    if config.is_json() {
        let report = GenerationReport {
            provider: provider.to_owned(),
            action: "generated".into(),
            output: stdout.trim().to_owned(),
        };
        println!("{}", serde_json::to_string(&report).unwrap());
    } else {
        println!(
            "{}",
            format::summary_line(
                config,
                &format!(
                    "{} Adapter Generated  {}",
                    symbols::pass_symbol(config),
                    provider
                )
            )
        );
        println!();
        if !stdout.is_empty() {
            for line in stdout.trim().lines() {
                println!("{}", format::detail_line(line, 2));
            }
        }
        println!();
        println!(
            "{}",
            format::help_line(
                config,
                &format!("Run `payrail conformance run {}` to validate", provider)
            )
        );
    }

    // Exit code 2 if the generation output contains VERIFY items
    let exit_code = if stdout.contains("VERIFY") { 2 } else { 0 };
    Ok(exit_code)
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
    fn generate_rejects_missing_knowledge_pack() {
        let result = generate_adapter("nonexistent-provider", &test_config());
        assert!(result.is_err());
        match result.unwrap_err() {
            GenerateError::KnowledgePackNotFound(p) => assert_eq!(p, "nonexistent-provider"),
            other => panic!("expected KnowledgePackNotFound, got: {other}"),
        }
    }

    #[test]
    fn format_error_text_contains_parts() {
        let cfg = test_config();
        let err = GenerateError::KnowledgePackNotFound("test".to_owned());
        let formatted = format_error(&cfg, &err);
        assert!(formatted.contains("knowledge pack"));
        assert!(formatted.contains("test"));
    }

    #[test]
    fn format_error_json_is_valid() {
        let cfg = json_config();
        let err = GenerateError::KnowledgePackNotFound("test".to_owned());
        let formatted = format_error(&cfg, &err);
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["status"], "error");
    }

    #[test]
    fn error_display_formats() {
        let err = GenerateError::ScriptNotFound;
        let msg = err.to_string();
        assert!(msg.contains("GENERATE_SCRIPT_MISSING"));

        let err = GenerateError::NodeNotFound;
        assert!(err.to_string().contains("Node.js"));
    }
}
