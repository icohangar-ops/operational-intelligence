use anyhow::Result;
use clap::{Parser, Subcommand};
use oi_api::{default_state, serve};
use oi_connector::{DataConnector, MockAnalyticsConnector, OperationalQuery};
use oi_core::{WorkflowKind, WorkflowRecord};
use oi_crew::{ContentCrewConfig, ContentCrewOrchestrator};
use oi_hiring::HiringAnalyzer;
use oi_llm::MockLlm;
use oi_memory::{default_store_dir, SharedStore, WorkflowStore};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "oi", about = "Operational Intelligence platform", version)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[arg(long, env = "OI_LEDGER_SIGNING_KEY")]
    signing_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start REST API server
    Serve {
        #[arg(long, env = "OI_BIND_ADDR", default_value = "0.0.0.0:8090")]
        bind: String,
    },
    /// Run content crew: research → analyst → writer → editor
    Crew {
        topic: String,
        #[arg(long)]
        no_approval: bool,
    },
    /// Analyze interview transcript with evidence tracing
    Hiring {
        #[arg(long)]
        file: PathBuf,
    },
    /// Query operational data models for initiative/ROI insights
    Query {
        question: String,
    },
    /// List persisted workflows
    Workflows,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = open_store(&cli.root)?;

    match cli.command {
        Commands::Serve { bind } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();
            let state = default_state(store);
            tokio::runtime::Runtime::new()?.block_on(serve(&bind, state))?;
        }
        Commands::Crew { topic, no_approval } => {
            let workflow = WorkflowRecord::new(WorkflowKind::ContentCrew, topic);
            let orchestrator = ContentCrewOrchestrator::new(
                Arc::new(MockLlm),
                store,
                ContentCrewConfig {
                    require_approval: !no_approval,
                    signing_key: cli.signing_key,
                },
            );
            let output = tokio::runtime::Runtime::new()?.block_on(orchestrator.run(workflow))?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Commands::Hiring { file } => {
            let content = std::fs::read_to_string(&file)?;
            let lines = HiringAnalyzer::parse_transcript(&content);
            let workflow = WorkflowRecord::new(WorkflowKind::HiringAnalysis, file.display().to_string());
            let analyzer = HiringAnalyzer::new(Arc::new(MockLlm), store, cli.signing_key);
            let assessment = tokio::runtime::Runtime::new()?
                .block_on(analyzer.analyze(workflow, &lines))?;
            println!("{}", serde_json::to_string_pretty(&assessment)?);
        }
        Commands::Query { question } => {
            let connector = MockAnalyticsConnector;
            let insight = tokio::runtime::Runtime::new()?.block_on(connector.query(OperationalQuery {
                question,
                initiative_id: None,
            }))?;
            println!("{}", serde_json::to_string_pretty(&insight)?);
        }
        Commands::Workflows => {
            let workflows = store.list()?;
            println!("{}", serde_json::to_string_pretty(&workflows)?);
        }
    }

    Ok(())
}

fn open_store(root: &PathBuf) -> Result<SharedStore> {
    Ok(Arc::new(WorkflowStore::with_persistence(default_store_dir(root))?))
}
