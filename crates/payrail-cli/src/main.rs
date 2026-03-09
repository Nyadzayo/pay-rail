mod knowledge;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "payrail", about = "PayRail CLI — payment processing toolkit")]
struct Cli {
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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Knowledge { action } => match action {
            KnowledgeAction::Init {
                provider_name,
                base_dir,
            } => {
                if let Err(e) = knowledge::init_knowledge_pack(&base_dir, &provider_name) {
                    eprintln!("{e}");
                    std::process::exit(1);
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
                ) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            KnowledgeAction::Validate {
                provider,
                pack,
                sandbox,
            } => {
                if let Err(e) = knowledge::validate_sandbox(&provider, &pack, sandbox) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            KnowledgeAction::Compile {
                provider,
                pack,
                budget,
                config,
            } => {
                if let Err(e) = knowledge::compile_pack(&provider, &pack, budget, config.as_deref())
                {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        },
    }
}
