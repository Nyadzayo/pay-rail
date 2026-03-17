use indicatif::{ProgressBar, ProgressStyle};
use payrail_core::knowledge::KnowledgePack;
use payrail_output::config::OutputConfig;
use payrail_output::{colors, format, symbols};
use serde::Serialize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Result of a knowledge pack init operation.
#[derive(Serialize)]
struct InitReport {
    provider: String,
    action: String,
    sections: Vec<String>,
    path: String,
}

/// Result of a knowledge pack validate operation.
#[derive(Serialize)]
struct ValidateReport {
    provider: String,
    action: String,
    output: String,
}

/// Result of a knowledge pack compile operation.
#[derive(Serialize)]
struct CompileReport {
    provider: String,
    action: String,
    output: String,
}

/// Initialize a knowledge pack scaffold for a payment provider.
pub fn init_knowledge_pack(
    base_dir: &Path,
    provider_name: &str,
    config: &OutputConfig,
) -> Result<(), KnowledgeScaffoldError> {
    validate_provider_name(provider_name)?;

    let provider_dir = base_dir.join(provider_name);

    if provider_dir.join("pack.yaml").exists() {
        return Err(KnowledgeScaffoldError::AlreadyExists(
            provider_name.to_owned(),
        ));
    }

    // Create directory structure
    fs::create_dir_all(provider_dir.join("compiled"))
        .map_err(|e| KnowledgeScaffoldError::Io(e.to_string()))?;
    fs::create_dir_all(provider_dir.join("tests"))
        .map_err(|e| KnowledgeScaffoldError::Io(e.to_string()))?;

    // Generate display name from kebab-case: "peach-payments" -> "Peach Payments"
    let display_name: String = provider_name
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let pack = KnowledgePack::scaffold(provider_name, &display_name);
    let yaml = generate_commented_yaml(&pack)?;

    fs::write(provider_dir.join("pack.yaml"), yaml)
        .map_err(|e| KnowledgeScaffoldError::Io(e.to_string()))?;

    let sections = vec![
        "endpoints".into(),
        "webhooks".into(),
        "status_codes".into(),
        "errors".into(),
        "flows".into(),
    ];

    if config.is_json() {
        let report = InitReport {
            provider: provider_name.to_owned(),
            action: "initialized".into(),
            sections: sections.clone(),
            path: provider_dir.display().to_string(),
        };
        println!("{}", serde_json::to_string(&report).unwrap());
    } else {
        println!(
            "{}",
            format::summary_line(
                config,
                &format!(
                    "{} Knowledge Pack  {}",
                    symbols::pass_symbol(config),
                    provider_name
                )
            )
        );
        println!();
        println!("{}", format::section_header(config, "Action: initialized"));
        println!(
            "{}",
            format::detail_line(&format!("Sections: {}", sections.join(", ")), 2)
        );
        println!(
            "{}",
            format::detail_line(&format!("Path: {}", provider_dir.display()), 2)
        );
        println!();
        println!(
            "{}",
            format::help_line(
                config,
                &format!(
                    "Run `payrail knowledge ingest {}` to add documentation",
                    provider_name
                )
            )
        );
    }

    Ok(())
}

/// Generate YAML with inline comments explaining each section.
fn generate_commented_yaml(pack: &KnowledgePack) -> Result<String, KnowledgeScaffoldError> {
    let raw_yaml = serde_yaml::to_string(pack)
        .map_err(|e| KnowledgeScaffoldError::Serialization(e.to_string()))?;

    let commented = raw_yaml
        .replace(
            "\nendpoints:",
            "\n# API endpoints: URL, method, parameters, response schema\n# Each entry is wrapped in a FactEntry with confidence metadata\nendpoints:",
        )
        .replace(
            "\nwebhooks:",
            "\n# Webhook event types: event name, payload schema, trigger conditions\nwebhooks:",
        )
        .replace(
            "\nstatus_codes:",
            "\n# Status code mappings: provider code -> canonical PayRail state\nstatus_codes:",
        )
        .replace(
            "\nerrors:",
            "\n# Error codes: provider error code, description, recovery action\nerrors:",
        )
        .replace(
            "\nflows:",
            "\n# Payment flow sequences: ordered steps for each payment flow\nflows:",
        );

    Ok(commented)
}

#[derive(Debug)]
pub enum KnowledgeScaffoldError {
    InvalidName(String),
    InvalidSourceType(String),
    AlreadyExists(String),
    Io(String),
    Serialization(String),
}

impl std::fmt::Display for KnowledgeScaffoldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(name) => write!(
                f,
                "[KNOWLEDGE_INVALID_NAME] Provider name '{name}' is not valid kebab-case [Use lowercase letters, digits, and hyphens only, starting with a letter]"
            ),
            Self::InvalidSourceType(t) => write!(
                f,
                "[KNOWLEDGE_INVALID_SOURCE_TYPE] Source type '{t}' is not valid [Use one of: official, community, historical, sandbox, inferred]"
            ),
            Self::AlreadyExists(name) => write!(
                f,
                "[KNOWLEDGE_SCAFFOLD_EXISTS] Provider '{name}' already has a pack.yaml [Use a different provider name or remove existing pack]"
            ),
            Self::Io(msg) => write!(f, "[KNOWLEDGE_IO] {msg}"),
            Self::Serialization(msg) => write!(
                f,
                "[KNOWLEDGE_SCAFFOLD_SERIAL] Serialization error: {msg} [Report this as a bug]"
            ),
        }
    }
}

impl std::error::Error for KnowledgeScaffoldError {}

/// Format a KnowledgeScaffoldError using the OutputConfig.
pub fn format_error(config: &OutputConfig, err: &KnowledgeScaffoldError) -> String {
    let (what, why, action) = match err {
        KnowledgeScaffoldError::InvalidName(name) => (
            "Invalid provider name".to_owned(),
            format!("'{name}' is not valid kebab-case"),
            "Use lowercase letters, digits, and hyphens only, starting with a letter".to_owned(),
        ),
        KnowledgeScaffoldError::InvalidSourceType(t) => (
            "Invalid source type".to_owned(),
            format!("'{t}' is not recognized"),
            "Use one of: official, community, historical, sandbox, inferred".to_owned(),
        ),
        KnowledgeScaffoldError::AlreadyExists(name) => (
            "Scaffold exists".to_owned(),
            format!("Provider '{name}' already has a pack.yaml"),
            "Use a different provider name or remove existing pack".to_owned(),
        ),
        KnowledgeScaffoldError::Io(msg) => (
            "I/O error".to_owned(),
            msg.clone(),
            "Check file paths and permissions".to_owned(),
        ),
        KnowledgeScaffoldError::Serialization(msg) => (
            "Serialization error".to_owned(),
            msg.clone(),
            "Report this as a bug".to_owned(),
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

/// Validate that a provider name is kebab-case.
fn validate_provider_name(name: &str) -> Result<(), KnowledgeScaffoldError> {
    if name.len() < 2
        || !name
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_lowercase())
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || name.ends_with('-')
        || name.contains("--")
    {
        return Err(KnowledgeScaffoldError::InvalidName(name.to_owned()));
    }
    Ok(())
}

/// Create a spinner with the standard PayRail style.
fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_owned());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Run a TypeScript CLI command with optional spinner.
fn run_ts_command(
    cmd: &mut Command,
    config: &OutputConfig,
    spinner_message: &str,
) -> Result<(String, String), KnowledgeScaffoldError> {
    let spinner = if !config.is_json() {
        Some(create_spinner(spinner_message))
    } else {
        None
    };

    let output = cmd.output().map_err(|e| {
        if let Some(s) = &spinner {
            s.finish_and_clear();
        }
        if e.kind() == std::io::ErrorKind::NotFound {
            KnowledgeScaffoldError::Io(
                "Node.js is not installed or not in PATH [Install Node.js v20+ from nodejs.org]"
                    .to_owned(),
            )
        } else {
            KnowledgeScaffoldError::Io(format!(
                "Failed to spawn pipeline: {e} [Check Node.js installation]"
            ))
        }
    })?;

    if let Some(s) = &spinner {
        s.finish_and_clear();
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let detail = if stderr.is_empty() {
            "unknown error".to_owned()
        } else {
            stderr.trim().to_owned()
        };
        return Err(KnowledgeScaffoldError::Io(format!(
            "Pipeline failed: {detail}"
        )));
    }

    Ok((stdout, stderr))
}

/// Ingest provider documentation by delegating to the TypeScript ingestion pipeline.
pub fn ingest_documentation(
    provider: &str,
    source_path: &Path,
    source_type: &str,
    pack_path: Option<&Path>,
    config: &OutputConfig,
) -> Result<(), KnowledgeScaffoldError> {
    validate_provider_name(provider)?;

    let valid_types = ["official", "community", "historical", "sandbox", "inferred"];
    if !valid_types.contains(&source_type) {
        return Err(KnowledgeScaffoldError::InvalidSourceType(
            source_type.to_owned(),
        ));
    }

    if !source_path.exists() {
        return Err(KnowledgeScaffoldError::Io(format!(
            "Source file not found: {} [Provide a path to a documentation file]",
            source_path.display()
        )));
    }

    let script_path = Path::new("packages/mcp-server/dist/cli/ingest.js");
    if !script_path.exists() {
        return Err(KnowledgeScaffoldError::Io(
            "Ingestion script not found at packages/mcp-server/dist/cli/ingest.js \
            [Run 'pnpm --filter @payrail/mcp-server build' first]"
                .to_owned(),
        ));
    }

    let mut cmd = Command::new("node");
    cmd.arg(script_path)
        .arg(provider)
        .arg("--source")
        .arg(source_path)
        .arg("--type")
        .arg(source_type);

    if let Some(pack) = pack_path {
        cmd.arg("--pack").arg(pack);
    }

    if config.is_json() {
        cmd.arg("--json");
    }

    let (stdout, _) = run_ts_command(
        &mut cmd,
        config,
        &format!("Ingesting documentation for {provider}..."),
    )?;

    if config.is_json() {
        let report = serde_json::json!({
            "provider": provider,
            "action": "ingested",
            "source": source_path.display().to_string(),
            "source_type": source_type,
            "output": stdout.trim(),
        });
        println!("{}", serde_json::to_string(&report).unwrap());
    } else if !stdout.is_empty() {
        println!(
            "{}",
            format::summary_line(
                config,
                &format!(
                    "{} Knowledge Pack  {}",
                    symbols::pass_symbol(config),
                    provider
                )
            )
        );
        println!();
        println!("{}", format::section_header(config, "Action: ingested"));
        println!(
            "{}",
            format::detail_line(&format!("Source: {}", source_path.display()), 2)
        );
        println!(
            "{}",
            format::detail_line(&format!("Type: {source_type}"), 2)
        );
        println!();
        print!("{stdout}");
    }

    Ok(())
}

/// Validate knowledge pack against provider's sandbox API.
pub fn validate_sandbox(
    provider: &str,
    pack_path: &Path,
    sandbox: bool,
    config: &OutputConfig,
) -> Result<(), KnowledgeScaffoldError> {
    validate_provider_name(provider)?;

    if !sandbox {
        return Err(KnowledgeScaffoldError::Io(
            "--sandbox flag is required for validation [Use: payrail knowledge validate --sandbox <provider> --pack <path>]"
                .to_owned(),
        ));
    }

    if !pack_path.exists() {
        return Err(KnowledgeScaffoldError::Io(format!(
            "Pack file not found: {} [Provide a path to a pack.yaml file]",
            pack_path.display()
        )));
    }

    let script_path = Path::new("packages/mcp-server/dist/cli/validate.js");
    if !script_path.exists() {
        return Err(KnowledgeScaffoldError::Io(
            "Validation script not found at packages/mcp-server/dist/cli/validate.js \
            [Run 'pnpm --filter @payrail/mcp-server build' first]"
                .to_owned(),
        ));
    }

    let mut cmd = Command::new("node");
    cmd.arg(script_path)
        .arg(provider)
        .arg("--pack")
        .arg(pack_path)
        .arg("--sandbox");

    if config.is_json() {
        cmd.arg("--json");
    }

    let (stdout, _) = run_ts_command(
        &mut cmd,
        config,
        &format!("Validating {provider} against sandbox..."),
    )?;

    if config.is_json() {
        let report = ValidateReport {
            provider: provider.to_owned(),
            action: "validated".into(),
            output: stdout.trim().to_owned(),
        };
        println!("{}", serde_json::to_string(&report).unwrap());
    } else {
        println!(
            "{}",
            format::summary_line(
                config,
                &format!(
                    "{} Knowledge Pack  {}",
                    symbols::pass_symbol(config),
                    provider
                )
            )
        );
        println!();
        println!("{}", format::section_header(config, "Action: validated"));
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
                &format!(
                    "Run `payrail knowledge compile {}` to produce versioned artifact",
                    provider
                )
            )
        );
    }

    Ok(())
}

/// Compile knowledge pack into optimized JSON.
pub fn compile_pack(
    provider: &str,
    pack_path: &Path,
    budget: Option<u32>,
    config_path: Option<&Path>,
    config: &OutputConfig,
) -> Result<(), KnowledgeScaffoldError> {
    validate_provider_name(provider)?;

    if !pack_path.exists() {
        return Err(KnowledgeScaffoldError::Io(format!(
            "Pack file not found: {} [Provide a path to a pack.yaml file]",
            pack_path.display()
        )));
    }

    let script_path = Path::new("packages/mcp-server/dist/cli/compile.js");
    if !script_path.exists() {
        return Err(KnowledgeScaffoldError::Io(
            "Compile script not found at packages/mcp-server/dist/cli/compile.js \
            [Run 'pnpm --filter @payrail/mcp-server build' first]"
                .to_owned(),
        ));
    }

    let mut cmd = Command::new("node");
    cmd.arg(script_path)
        .arg(provider)
        .arg("--pack")
        .arg(pack_path);

    if let Some(b) = budget {
        cmd.arg("--budget").arg(b.to_string());
    }

    if let Some(cfg) = config_path {
        cmd.arg("--config").arg(cfg);
    }

    if config.is_json() {
        cmd.arg("--json");
    }

    let (stdout, _) = run_ts_command(
        &mut cmd,
        config,
        &format!("Compiling knowledge pack for {provider}..."),
    )?;

    if config.is_json() {
        let report = CompileReport {
            provider: provider.to_owned(),
            action: "compiled".into(),
            output: stdout.trim().to_owned(),
        };
        println!("{}", serde_json::to_string(&report).unwrap());
    } else {
        println!(
            "{}",
            format::summary_line(
                config,
                &format!(
                    "{} Knowledge Pack  {}",
                    symbols::pass_symbol(config),
                    provider
                )
            )
        );
        println!();
        println!("{}", format::section_header(config, "Action: compiled"));
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
                &format!(
                    "Run `payrail knowledge publish {}` to share with community",
                    provider
                )
            )
        );
    }

    Ok(())
}

/// Publish a knowledge pack to the community registry.
pub fn publish_pack(
    provider: &str,
    pack_path: &Path,
    yes: bool,
    config: &OutputConfig,
) -> Result<(), KnowledgeScaffoldError> {
    validate_provider_name(provider)?;

    if !pack_path.exists() {
        return Err(KnowledgeScaffoldError::Io(format!(
            "Pack file not found: {} [Provide a path to a pack.yaml file]",
            pack_path.display()
        )));
    }

    if !yes {
        if config.is_json() {
            return Err(KnowledgeScaffoldError::Io(
                "Publish requires --yes flag in JSON/non-interactive mode [Use: payrail knowledge publish --yes]"
                    .to_owned(),
            ));
        }

        println!(
            "{}",
            colors::warning(
                config,
                &format!("Publishing {provider} to community registry...")
            )
        );
        println!();
        print!(
            "{}",
            colors::warning(config, "Are you sure? This action is irreversible. [y/N] ")
        );
        io::stdout().flush().ok();

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| KnowledgeScaffoldError::Io(format!("Failed to read input: {e}")))?;

        if input.trim().to_lowercase() != "y" {
            println!("{}", colors::info(config, "Publish cancelled."));
            return Ok(());
        }
    }

    // Publish is not yet implemented — registry doesn't exist yet
    Err(KnowledgeScaffoldError::Io(
        "Community registry not yet available [Publish will be available in a future release]"
            .to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("payrail-test-{}", ulid::Ulid::new()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_config() -> OutputConfig {
        // Use no-color, quiet config for tests to avoid terminal output noise
        OutputConfig::from_env(false, true, false, true)
    }

    fn json_config() -> OutputConfig {
        OutputConfig::from_env(true, false, false, false)
    }

    #[test]
    fn init_creates_directory_structure() {
        let base = temp_dir();
        init_knowledge_pack(&base, "test-provider", &test_config()).unwrap();

        assert!(base.join("test-provider/pack.yaml").exists());
        assert!(base.join("test-provider/compiled").is_dir());
        assert!(base.join("test-provider/tests").is_dir());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_creates_valid_yaml() {
        let base = temp_dir();
        init_knowledge_pack(&base, "test-provider", &test_config()).unwrap();

        let content = fs::read_to_string(base.join("test-provider/pack.yaml")).unwrap();
        let pack: KnowledgePack = serde_yaml::from_str(&content).unwrap();
        assert_eq!(pack.metadata.name, "test-provider");
        assert_eq!(pack.metadata.display_name, "Test Provider");
        assert!(pack.endpoints.is_empty());
        assert!(pack.webhooks.is_empty());
        assert!(pack.status_codes.is_empty());
        assert!(pack.errors.is_empty());
        assert!(pack.flows.is_empty());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_yaml_has_comments() {
        let base = temp_dir();
        init_knowledge_pack(&base, "test-provider", &test_config()).unwrap();

        let content = fs::read_to_string(base.join("test-provider/pack.yaml")).unwrap();
        assert!(content.contains("# API endpoints"));
        assert!(content.contains("# Webhook event types"));
        assert!(content.contains("# Status code mappings"));
        assert!(content.contains("# Error codes"));
        assert!(content.contains("# Payment flow sequences"));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_rejects_existing_pack() {
        let base = temp_dir();
        init_knowledge_pack(&base, "test-provider", &test_config()).unwrap();
        let result = init_knowledge_pack(&base, "test-provider", &test_config());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("KNOWLEDGE_SCAFFOLD_EXISTS"));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn display_name_from_kebab_case() {
        let base = temp_dir();
        init_knowledge_pack(&base, "peach-payments", &test_config()).unwrap();

        let content = fs::read_to_string(base.join("peach-payments/pack.yaml")).unwrap();
        let pack: KnowledgePack = serde_yaml::from_str(&content).unwrap();
        assert_eq!(pack.metadata.display_name, "Peach Payments");

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_rejects_path_traversal() {
        let base = temp_dir();
        let result = init_knowledge_pack(&base, "../../../etc", &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_rejects_uppercase_name() {
        let base = temp_dir();
        let result = init_knowledge_pack(&base, "Peach-Payments", &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_rejects_slashes() {
        let base = temp_dir();
        let result = init_knowledge_pack(&base, "foo/bar", &test_config());
        assert!(result.is_err());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_rejects_trailing_hyphen() {
        let base = temp_dir();
        let result = init_knowledge_pack(&base, "test-", &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_rejects_consecutive_hyphens() {
        let base = temp_dir();
        let result = init_knowledge_pack(&base, "test--provider", &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_rejects_single_char_name() {
        let base = temp_dir();
        let result = init_knowledge_pack(&base, "a", &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn pack_yaml_contains_no_secrets() {
        let base = temp_dir();
        init_knowledge_pack(&base, "test-provider", &test_config()).unwrap();

        let content = fs::read_to_string(base.join("test-provider/pack.yaml")).unwrap();
        let lower = content.to_lowercase();
        assert!(!lower.contains("api_key"));
        assert!(!lower.contains("secret"));
        assert!(!lower.contains("credential"));
        assert!(!lower.contains("password"));

        fs::remove_dir_all(&base).ok();
    }

    // --- ingest_documentation tests ---

    #[test]
    fn ingest_rejects_invalid_provider_name() {
        let base = temp_dir();
        let source = base.join("docs.md");
        fs::write(&source, "# Test").unwrap();
        let result =
            ingest_documentation("../../../etc", &source, "official", None, &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ingest_rejects_uppercase_provider() {
        let base = temp_dir();
        let source = base.join("docs.md");
        fs::write(&source, "# Test").unwrap();
        let result = ingest_documentation("BadName", &source, "official", None, &test_config());
        assert!(result.is_err());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ingest_rejects_invalid_source_type() {
        let base = temp_dir();
        let source = base.join("docs.md");
        fs::write(&source, "# Test").unwrap();
        let result = ingest_documentation(
            "test-provider",
            &source,
            "invalid_type",
            None,
            &test_config(),
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_SOURCE_TYPE")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ingest_rejects_missing_source_file() {
        let result = ingest_documentation(
            "test-provider",
            Path::new("/nonexistent/file.md"),
            "official",
            None,
            &test_config(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn ingest_rejects_missing_node_script() {
        let base = temp_dir();
        let source = base.join("docs.md");
        fs::write(&source, "# Test docs\nPOST /v1/payments").unwrap();
        let result =
            ingest_documentation("test-provider", &source, "official", None, &test_config());
        assert!(result.is_err());
        fs::remove_dir_all(&base).ok();
    }

    // --- validate_sandbox tests ---

    #[test]
    fn validate_rejects_invalid_provider_name() {
        let base = temp_dir();
        let pack = base.join("pack.yaml");
        fs::write(&pack, "metadata:\n  name: test").unwrap();
        let result = validate_sandbox("../../../etc", &pack, true, &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn validate_rejects_missing_sandbox_flag() {
        let base = temp_dir();
        let pack = base.join("pack.yaml");
        fs::write(&pack, "metadata:\n  name: test").unwrap();
        let result = validate_sandbox("test-provider", &pack, false, &test_config());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--sandbox"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn validate_rejects_missing_pack_file() {
        let result = validate_sandbox(
            "test-provider",
            Path::new("/nonexistent/pack.yaml"),
            true,
            &test_config(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    // --- compile_pack tests ---

    #[test]
    fn compile_rejects_invalid_provider_name() {
        let base = temp_dir();
        let pack = base.join("pack.yaml");
        fs::write(&pack, "metadata:\n  name: test").unwrap();
        let result = compile_pack("../../../etc", &pack, None, None, &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn compile_rejects_missing_pack_file() {
        let result = compile_pack(
            "test-provider",
            Path::new("/nonexistent/pack.yaml"),
            None,
            None,
            &test_config(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    // --- publish_pack tests ---

    #[test]
    fn publish_rejects_invalid_provider_name() {
        let base = temp_dir();
        let pack = base.join("pack.yaml");
        fs::write(&pack, "metadata:\n  name: test").unwrap();
        let result = publish_pack("../../../etc", &pack, true, &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("KNOWLEDGE_INVALID_NAME")
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn publish_rejects_missing_pack_file() {
        let result = publish_pack(
            "test-provider",
            Path::new("/nonexistent/pack.yaml"),
            true,
            &test_config(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn publish_requires_yes_in_json_mode() {
        let base = temp_dir();
        let pack = base.join("pack.yaml");
        fs::write(&pack, "metadata:\n  name: test").unwrap();
        let result = publish_pack("test-provider", &pack, false, &json_config());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--yes"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn publish_with_yes_returns_not_available() {
        let base = temp_dir();
        let pack = base.join("pack.yaml");
        fs::write(&pack, "metadata:\n  name: test").unwrap();
        let result = publish_pack("test-provider", &pack, true, &test_config());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not yet available")
        );
        fs::remove_dir_all(&base).ok();
    }

    // --- format_error tests ---

    #[test]
    fn format_error_text_contains_all_parts() {
        let cfg = test_config();
        let err = KnowledgeScaffoldError::InvalidName("BAD".to_owned());
        let formatted = format_error(&cfg, &err);
        assert!(formatted.contains("Invalid provider name"));
        assert!(formatted.contains("BAD"));
    }

    #[test]
    fn format_error_json_is_valid() {
        let cfg = json_config();
        let err = KnowledgeScaffoldError::InvalidName("BAD".to_owned());
        let formatted = format_error(&cfg, &err);
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["status"], "error");
        assert!(parsed["error"].as_str().unwrap().contains("Invalid"));
    }

    // --- init JSON output test ---

    #[test]
    fn init_json_output_is_valid() {
        let base = temp_dir();
        // We can't easily capture stdout in a unit test, but we can verify
        // the function succeeds with json config
        let result = init_knowledge_pack(&base, "json-test", &json_config());
        assert!(result.is_ok());

        // Verify the scaffold was actually created
        assert!(base.join("json-test/pack.yaml").exists());
        fs::remove_dir_all(&base).ok();
    }
}
