use payrail_core::knowledge::KnowledgePack;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Initialize a knowledge pack scaffold for a payment provider.
///
/// Creates the directory structure:
/// ```text
/// knowledge-packs/{provider}/
///   pack.yaml               # Source (human-authored)
///   compiled/                # Machine-consumed artifacts
///   tests/                   # Sandbox validation tests
/// ```
pub fn init_knowledge_pack(
    base_dir: &Path,
    provider_name: &str,
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

    println!(
        "Created {provider_name}/ with sections: endpoints, webhooks, status_codes, errors, flows"
    );

    Ok(())
}

/// Generate YAML with inline comments explaining each section.
fn generate_commented_yaml(pack: &KnowledgePack) -> Result<String, KnowledgeScaffoldError> {
    let raw_yaml = serde_yaml::to_string(pack)
        .map_err(|e| KnowledgeScaffoldError::Serialization(e.to_string()))?;

    // Insert comments before each major section
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

/// Validate that a provider name is kebab-case.
fn validate_provider_name(name: &str) -> Result<(), KnowledgeScaffoldError> {
    if !name
        .as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_lowercase())
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(KnowledgeScaffoldError::InvalidName(name.to_owned()));
    }
    Ok(())
}

/// Ingest provider documentation by delegating to the TypeScript ingestion pipeline.
pub fn ingest_documentation(
    provider: &str,
    source_path: &Path,
    source_type: &str,
    pack_path: Option<&Path>,
) -> Result<(), KnowledgeScaffoldError> {
    // Validate provider name (same rules as init)
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

    // Locate the TypeScript ingestion script
    let script_path = Path::new("packages/mcp-server/dist/cli/ingest.js");
    if !script_path.exists() {
        return Err(KnowledgeScaffoldError::Io(
            "Ingestion script not found at packages/mcp-server/dist/cli/ingest.js \
            [Run 'pnpm --filter @payrail/mcp-server build' first]"
                .to_owned(),
        ));
    }

    // Delegate to the TypeScript CLI
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

    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            KnowledgeScaffoldError::Io(
                "Node.js is not installed or not in PATH [Install Node.js v20+ from nodejs.org]"
                    .to_owned(),
            )
        } else {
            KnowledgeScaffoldError::Io(format!(
                "Failed to spawn ingestion pipeline: {e} [Check Node.js installation]"
            ))
        }
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        print!("{stdout}");
    }

    if !output.status.success() {
        let detail = if stderr.is_empty() {
            "unknown error".to_owned()
        } else {
            stderr.trim().to_owned()
        };
        return Err(KnowledgeScaffoldError::Io(format!(
            "Ingestion pipeline failed: {detail} [Check source file format]"
        )));
    }

    Ok(())
}

/// Validate knowledge pack against provider's sandbox API by delegating to the TypeScript pipeline.
pub fn validate_sandbox(
    provider: &str,
    pack_path: &Path,
    sandbox: bool,
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

    let output = Command::new("node")
        .arg(script_path)
        .arg(provider)
        .arg("--pack")
        .arg(pack_path)
        .arg("--sandbox")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                KnowledgeScaffoldError::Io(
                    "Node.js is not installed or not in PATH [Install Node.js v20+ from nodejs.org]"
                        .to_owned(),
                )
            } else {
                KnowledgeScaffoldError::Io(format!(
                    "Failed to spawn validation pipeline: {e} [Check Node.js installation]"
                ))
            }
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        print!("{stdout}");
    }

    if !output.status.success() {
        let detail = if stderr.is_empty() {
            "unknown error".to_owned()
        } else {
            stderr.trim().to_owned()
        };
        return Err(KnowledgeScaffoldError::Io(format!(
            "Validation pipeline failed: {detail} [Check sandbox credentials and pack file]"
        )));
    }

    Ok(())
}

/// Compile knowledge pack into optimized JSON by delegating to the TypeScript pipeline.
pub fn compile_pack(
    provider: &str,
    pack_path: &Path,
    budget: Option<u32>,
    config_path: Option<&Path>,
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

    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            KnowledgeScaffoldError::Io(
                "Node.js is not installed or not in PATH [Install Node.js v20+ from nodejs.org]"
                    .to_owned(),
            )
        } else {
            KnowledgeScaffoldError::Io(format!(
                "Failed to spawn compilation pipeline: {e} [Check Node.js installation]"
            ))
        }
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        print!("{stdout}");
    }

    if !output.status.success() {
        let detail = if stderr.is_empty() {
            "unknown error".to_owned()
        } else {
            stderr.trim().to_owned()
        };
        return Err(KnowledgeScaffoldError::Io(format!(
            "Compilation pipeline failed: {detail} [Check pack file format]"
        )));
    }

    Ok(())
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

    #[test]
    fn init_creates_directory_structure() {
        let base = temp_dir();
        init_knowledge_pack(&base, "test-provider").unwrap();

        assert!(base.join("test-provider/pack.yaml").exists());
        assert!(base.join("test-provider/compiled").is_dir());
        assert!(base.join("test-provider/tests").is_dir());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_creates_valid_yaml() {
        let base = temp_dir();
        init_knowledge_pack(&base, "test-provider").unwrap();

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
        init_knowledge_pack(&base, "test-provider").unwrap();

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
        init_knowledge_pack(&base, "test-provider").unwrap();
        let result = init_knowledge_pack(&base, "test-provider");

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("KNOWLEDGE_SCAFFOLD_EXISTS"));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn display_name_from_kebab_case() {
        let base = temp_dir();
        init_knowledge_pack(&base, "peach-payments").unwrap();

        let content = fs::read_to_string(base.join("peach-payments/pack.yaml")).unwrap();
        let pack: KnowledgePack = serde_yaml::from_str(&content).unwrap();
        assert_eq!(pack.metadata.display_name, "Peach Payments");

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn init_rejects_path_traversal() {
        let base = temp_dir();
        let result = init_knowledge_pack(&base, "../../../etc");
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
        let result = init_knowledge_pack(&base, "Peach-Payments");
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
        let result = init_knowledge_pack(&base, "foo/bar");
        assert!(result.is_err());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn pack_yaml_contains_no_secrets() {
        let base = temp_dir();
        init_knowledge_pack(&base, "test-provider").unwrap();

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
        let result = ingest_documentation("../../../etc", &source, "official", None);
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
        let result = ingest_documentation("BadName", &source, "official", None);
        assert!(result.is_err());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn ingest_rejects_invalid_source_type() {
        let base = temp_dir();
        let source = base.join("docs.md");
        fs::write(&source, "# Test").unwrap();
        let result = ingest_documentation("test-provider", &source, "invalid_type", None);
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
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn ingest_rejects_missing_node_script() {
        let base = temp_dir();
        let source = base.join("docs.md");
        fs::write(&source, "# Test docs\nPOST /v1/payments").unwrap();
        // This test will fail because ingest.js won't exist at cwd
        // It exercises the script-not-found check
        let result = ingest_documentation("test-provider", &source, "official", None);
        // Will error either because script not found or node not found
        assert!(result.is_err());
        fs::remove_dir_all(&base).ok();
    }

    // --- validate_sandbox tests ---

    #[test]
    fn validate_rejects_invalid_provider_name() {
        let base = temp_dir();
        let pack = base.join("pack.yaml");
        fs::write(&pack, "metadata:\n  name: test").unwrap();
        let result = validate_sandbox("../../../etc", &pack, true);
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
        let result = validate_sandbox("test-provider", &pack, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--sandbox"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn validate_rejects_missing_pack_file() {
        let result = validate_sandbox("test-provider", Path::new("/nonexistent/pack.yaml"), true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    // --- compile_pack tests ---

    #[test]
    fn compile_rejects_invalid_provider_name() {
        let base = temp_dir();
        let pack = base.join("pack.yaml");
        fs::write(&pack, "metadata:\n  name: test").unwrap();
        let result = compile_pack("../../../etc", &pack, None, None);
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
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
