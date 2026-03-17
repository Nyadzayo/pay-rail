mod conformance;
mod doctor;
mod generate;
mod init;
mod knowledge;
mod reconciliation;
mod status;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use payrail_output::OutputConfig;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "payrail",
    about = "PayRail CLI — payment processing toolkit",
    version,
    long_about = None,
)]
struct Cli {
    /// Output as JSON (machine-readable, no colors)
    #[arg(long, global = true)]
    json: bool,

    /// Enable verbose output
    #[arg(long, global = true, conflicts_with = "quiet")]
    verbose: bool,

    /// Suppress non-essential output
    #[arg(long, global = true)]
    quiet: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Knowledge pack management
    Knowledge {
        #[command(subcommand)]
        action: KnowledgeAction,
    },

    /// Generate a provider adapter from a knowledge pack
    Generate {
        /// Provider name in kebab-case
        provider: String,
    },

    /// Conformance testing for provider adapters
    Conformance {
        #[command(subcommand)]
        action: ConformanceAction,
    },

    /// Initialize a new PayRail project
    Init {
        /// Provider name in kebab-case
        #[arg(long)]
        provider: Option<String>,

        /// Language (auto-detected if not specified)
        #[arg(long)]
        lang: Option<String>,

        /// Framework (auto-detected if not specified)
        #[arg(long)]
        framework: Option<String>,
    },

    /// Check project health and configuration
    Doctor,

    /// Show project status and provider summary
    Status {
        /// Time period for stats (default: 24h)
        #[arg(long, default_value = "24h", value_parser = ["1h", "12h", "24h", "7d"])]
        period: String,
    },

    /// Show reconciliation report for payment providers
    Reconciliation {
        /// Filter by provider name
        #[arg(long)]
        provider: Option<String>,

        /// Time period for report (default: 24h)
        #[arg(long, default_value = "24h", value_parser = ["1h", "12h", "24h", "7d"])]
        period: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum KnowledgeAction {
    /// Initialize a knowledge pack scaffold for a payment provider
    Init {
        /// Provider name in kebab-case (e.g., peach-payments)
        provider_name: String,

        /// Base directory for knowledge packs (default: knowledge-packs/)
        #[arg(long, default_value = "knowledge-packs")]
        base_dir: PathBuf,
    },
    /// Ingest provider documentation into structured facts
    Ingest {
        /// Provider name in kebab-case (e.g., peach-payments)
        provider: String,

        /// Path to documentation source file
        #[arg(long)]
        source: PathBuf,

        /// Source type: official, community, historical, sandbox, inferred
        #[arg(long, name = "type")]
        source_type: String,

        /// Path to existing pack.yaml to merge with
        #[arg(long)]
        pack: Option<PathBuf>,
    },
    /// Validate knowledge pack against provider's sandbox API
    Validate {
        /// Provider name in kebab-case (e.g., peach-payments)
        provider: String,

        /// Path to pack.yaml to validate
        #[arg(long)]
        pack: PathBuf,

        /// Run validation against sandbox API
        #[arg(long)]
        sandbox: bool,
    },
    /// Compile knowledge pack into optimized JSON artifact
    Compile {
        /// Provider name in kebab-case (e.g., peach-payments)
        provider: String,

        /// Path to pack.yaml source file
        #[arg(long)]
        pack: PathBuf,

        /// Token budget override (default: from config or 8000)
        #[arg(long)]
        budget: Option<u32>,

        /// Path to payrail.config.yaml
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Publish knowledge pack to community registry
    Publish {
        /// Provider name in kebab-case (e.g., peach-payments)
        provider: String,

        /// Path to pack.yaml source file
        #[arg(long)]
        pack: PathBuf,

        /// Skip confirmation prompt (required for non-interactive/CI use)
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum ConformanceAction {
    /// Run conformance tests against a provider adapter
    Run {
        /// Provider name in kebab-case (e.g., peach-payments)
        provider: String,

        /// Execute against live sandbox API
        #[arg(long)]
        sandbox: bool,
    },
}

fn handle_knowledge_error(config: &OutputConfig, err: &knowledge::KnowledgeScaffoldError) -> ! {
    let formatted = knowledge::format_error(config, err);
    eprintln!("{formatted}");
    std::process::exit(1)
}

fn handle_generate_error(config: &OutputConfig, err: &generate::GenerateError) -> ! {
    let formatted = generate::format_error(config, err);
    eprintln!("{formatted}");
    std::process::exit(1)
}

fn handle_conformance_error(config: &OutputConfig, err: &conformance::ConformanceError) -> ! {
    let formatted = conformance::format_error(config, err);
    eprintln!("{formatted}");
    std::process::exit(1)
}

fn main() {
    let cli = Cli::parse();

    let config = OutputConfig::from_env(cli.json, cli.no_color, cli.verbose, cli.quiet);

    match cli.command {
        Commands::Knowledge { action } => match action {
            KnowledgeAction::Init {
                provider_name,
                base_dir,
            } => {
                if let Err(e) = knowledge::init_knowledge_pack(&base_dir, &provider_name, &config) {
                    handle_knowledge_error(&config, &e);
                }
            }
            KnowledgeAction::Ingest {
                provider,
                source,
                source_type,
                pack,
            } => {
                if let Err(e) = knowledge::ingest_documentation(
                    &provider,
                    &source,
                    &source_type,
                    pack.as_deref(),
                    &config,
                ) {
                    handle_knowledge_error(&config, &e);
                }
            }
            KnowledgeAction::Validate {
                provider,
                pack,
                sandbox,
            } => {
                if let Err(e) = knowledge::validate_sandbox(&provider, &pack, sandbox, &config) {
                    handle_knowledge_error(&config, &e);
                }
            }
            KnowledgeAction::Compile {
                provider,
                pack,
                budget,
                config: cfg_path,
            } => {
                if let Err(e) =
                    knowledge::compile_pack(&provider, &pack, budget, cfg_path.as_deref(), &config)
                {
                    handle_knowledge_error(&config, &e);
                }
            }
            KnowledgeAction::Publish {
                provider,
                pack,
                yes,
            } => {
                if let Err(e) = knowledge::publish_pack(&provider, &pack, yes, &config) {
                    handle_knowledge_error(&config, &e);
                }
            }
        },
        Commands::Generate { provider } => match generate::generate_adapter(&provider, &config) {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => handle_generate_error(&config, &e),
        },
        Commands::Conformance { action } => match action {
            ConformanceAction::Run { provider, sandbox } => {
                match conformance::conformance_run(&provider, sandbox, &config) {
                    Ok(exit_code) => std::process::exit(exit_code),
                    Err(e) => handle_conformance_error(&config, &e),
                }
            }
        },
        Commands::Init {
            provider,
            lang,
            framework,
        } => {
            if let Err(e) = init::init_project(
                provider.as_deref(),
                lang.as_deref(),
                framework.as_deref(),
                &config,
            ) {
                let formatted = init::format_error(&config, &e);
                eprintln!("{formatted}");
                std::process::exit(1);
            }
        }
        Commands::Doctor => {
            let checks = doctor::run_doctor(&config);
            let has_failures = checks.iter().any(|c| !c.passed);
            if has_failures {
                std::process::exit(1);
            }
        }
        Commands::Status { period } => {
            status::show_status(&period, &config);
        }
        Commands::Reconciliation { provider, period } => {
            reconciliation::show_reconciliation(provider.as_deref(), &period, &config);
        }
        Commands::Completions { shell } => {
            generate(
                shell,
                &mut Cli::command(),
                "payrail",
                &mut std::io::stdout(),
            );
        }
    }
}
