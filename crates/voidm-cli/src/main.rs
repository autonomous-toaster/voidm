use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use std::sync::Arc;

mod commands;
mod output;
mod instructions;

use voidm_core::Config;
use voidm_db::Database;

#[derive(Parser)]
#[command(name = "voidm", about = "Graph-native memory engine for LLM agents", version)]
pub struct Cli {
    /// Path to config file [env: VOIDM_CONFIG]
    #[arg(long, global = true, env = "VOIDM_CONFIG")]
    pub config: Option<String>,

    /// Output JSON (machine-readable)
    #[arg(long, global = true)]
    pub json: bool,

    /// Suppress decorative output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add a memory
    Add(commands::add::AddArgs),
    /// Get a memory by ID
    Get(commands::get::GetArgs),
    /// Hybrid search
    Search(commands::search::SearchArgs),
    /// List memories (newest first)
    List(commands::list::ListArgs),
    /// Delete a memory (cascades graph edges)
    Delete(commands::delete::DeleteArgs),
    /// Create a graph edge between two memories
    Link(commands::link::LinkArgs),
    /// Remove a graph edge
    Unlink(commands::unlink::UnlinkArgs),
    /// Initialize voidm: download and cache all models
    Init(commands::init::InitArgs),
    /// Graph operations
    #[command(subcommand)]
    Graph(commands::graph::GraphCommands),
    /// List all known scope strings
    #[command(subcommand)]
    Scopes(commands::scopes::ScopesCommands),
    /// Export memories
    Export(commands::export::ExportArgs),
    /// Show or edit config
    #[command(subcommand)]
    Config(commands::config::ConfigCommands),
    /// Model management
    #[command(subcommand)]
    Models(commands::models::ModelsCommands),
    /// Print usage guide for LLM agents
    Instructions(commands::instructions::InstructionsArgs),
    /// Show paths, config and runtime settings
    Info(commands::info::InfoArgs),
    /// Show memory and graph statistics
    Stats(commands::stats::StatsArgs),
    /// Check for new releases on GitHub
    CheckUpdate(commands::update::CheckUpdateArgs),
}

#[tokio::main]
async fn main() {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            let msg = if msg.contains("--type") && msg.contains("required arguments") {
                format!(
                    "{msg}\nValid memory types: episodic, semantic, procedural, conceptual, contextual\n\
                     Example: voidm add \"content\" --type semantic"
                )
            } else {
                msg
            };
            eprintln!("{msg}");
            std::process::exit(e.exit_code());
        }
    };

    let env_filter = match std::env::var("RUST_LOG") {
        Ok(log_env) => {
            if !log_env.contains("ort=") {
                format!("{},ort=off", log_env)
            } else {
                log_env
            }
        }
        Err(_) => "error,ort=off,ort::logging=off".to_string(),
    };
    
    let env_filter = EnvFilter::try_from(env_filter.as_str())
        .unwrap_or_else(|_| EnvFilter::new("info,ort=off,ort::logging=off"));
    
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();

    let json = cli.json;
    let result = run(cli).await;
    match result {
        Ok(()) => {}
        Err(e) => {
            emit_error(&e.to_string(), json);
            std::process::exit(2);
        }
    }
}

pub fn emit_error(msg: &str, json: bool) {
    if json {
        crate::output::print_error(msg);
    } else {
        eprintln!("Error: {msg}");
    }
}

/// Backend-agnostic database factory.
/// This is the ONLY place in the CLI that knows about concrete backend types.
async fn resolve_backend(config: &Config) -> Result<Arc<dyn Database>> {
    match config.database.backend.as_str() {
        "neo4j" => {
            let neo4j_config = config.database.neo4j.as_ref()
                .ok_or_else(|| anyhow::anyhow!("Neo4j config missing in config.toml"))?;
            let db = voidm_neo4j::Neo4jDatabase::connect(
                &neo4j_config.uri,
                &neo4j_config.username,
                &neo4j_config.password,
                &neo4j_config.database,
            ).await?;
            Ok(Arc::new(db) as Arc<dyn Database>)
        }
        #[cfg(feature = "database-sqlite")]
        "sqlite" => {
            let db_path = config.db_path(None);
            let pool = voidm_sqlite::open_pool(&db_path).await?;
            Ok(Arc::new(voidm_sqlite::SqliteDatabase::new(pool.clone())) as Arc<dyn Database>)
        }
        other => {
            anyhow::bail!("Unsupported backend: '{}'", other)
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    // Commands that don't need DB
    match &cli.command {
        Commands::Instructions(args) => {
            return commands::instructions::run(args, cli.json);
        }
        Commands::Config(cmd) => {
            return commands::config::run(cmd, cli.json).await;
        }
        Commands::Info(args) => {
            let config = Config::load_from(cli.config.as_deref());
            return commands::info::run(args.clone(), &config, None, cli.json);
        }
        Commands::Init(args) => {
            return commands::init::run(args.clone()).await;
        }
        Commands::CheckUpdate(args) => {
            return commands::update::check_update(args.clone()).await;
        }
        Commands::Models(cmd) => {
            if let commands::models::ModelsCommands::List = cmd {
                return commands::models::run_list(cli.json);
            }
        }
        _ => {}
    }

    let config = Config::load_from(cli.config.as_deref());
    config.validate_generation_backends()?;
    
    let db = resolve_backend(&config).await?;
    db.ensure_schema().await?;

    // Note: the CLI is intentionally shallow here — every command just dispatches
    // to the Database trait. No backend-specific logic leaks past this point.
    match cli.command {
        Commands::Add(args) => commands::add::run(args, &db, &config, cli.json).await,
        Commands::Get(args) => commands::get::run(args, &db, cli.json).await,
        Commands::Search(args) => commands::search::run(args, &db, &config, cli.json).await,
        Commands::List(args) => commands::list::run(args, &db, &config, cli.json).await,
        Commands::Delete(args) => commands::delete::run(args, &db, cli.json).await,
        Commands::Link(args) => commands::link::run(args, &db, cli.json).await,
        Commands::Unlink(args) => commands::unlink::run(args, &db, cli.json).await,
        Commands::Graph(cmd) => commands::graph::run(cmd, &db, cli.json).await,
        Commands::Scopes(cmd) => commands::scopes::run(cmd, &db, cli.json).await,
        Commands::Export(args) => commands::export::run(args, &db, &config, cli.json).await,
        Commands::Config(_) => unreachable!(),
        Commands::Models(cmd) => commands::models::run(cmd, cli.json).await,
        Commands::Instructions(_) => unreachable!(),
        Commands::Info(_) => unreachable!(),
        Commands::Init(_) => unreachable!(),
        Commands::CheckUpdate(_) => unreachable!(),
        Commands::Stats(args) => commands::stats::run(args, &db, &config, cli.json).await,
    }
}
