use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use std::sync::Arc;

mod commands;
mod output;
mod instructions;

use voidm_core::Config;
use voidm_sqlite::open_pool;

#[derive(Parser)]
#[command(name = "voidm", about = "Local-first memory tool for LLM agents", version)]
pub struct Cli {
    /// Override database path [env: VOIDM_DB]
    #[arg(long, global = true, env = "VOIDM_DB")]
    pub db: Option<String>,

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
    /// Ontology operations (concepts, hierarchy, instances)
    #[command(subcommand)]
    Ontology(commands::ontology::OntologyCommands),
    /// Review and resolve ontology conflicts (CONTRADICTS edges)
    #[command(subcommand)]
    Conflicts(commands::conflicts::ConflictsCommands),
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
    /// Run an assistant-friendly MCP server
    Mcp(commands::mcp::McpArgs),
    /// Migrate data between backends (sqlite ↔ neo4j)
    Migrate(commands::migrate::MigrateArgs),
    /// Check for new releases on GitHub
    CheckUpdate(commands::update::CheckUpdateArgs),
    /// Consolidate: unified memory cleanup & concept management
    Consolidate(commands::consolidate::ConsolidateArgs),
    /// Validate Phase A chunking algorithm on real data
    Validate(commands::validate::ValidationArgs),
    /// Part D: Chunk memories and store in Neo4j
    Chunk(commands::chunk::ChunkArgs),
}

#[tokio::main]
async fn main() {
    // Intercept clap parse errors to inject helpful hints for known args.
    // We parse manually so we can customise the error before clap exits.
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            // Augment missing --type with the list of valid types
            let msg = if msg.contains("--type") && msg.contains("required arguments") {
                format!(
                    "{msg}\nValid memory types: episodic, semantic, procedural, conceptual, contextual\n\
                     Example: voidm add \"content\" --type semantic"
                )
            } else {
                msg
            };
            // Print to stderr and exit with clap's own code (1 for usage, 2 for error)
            eprintln!("{msg}");
            std::process::exit(e.exit_code());
        }
    };

    // Logging: suppress noisy ORT logs by default, but allow user to override
    let env_filter = match std::env::var("RUST_LOG") {
        Ok(log_env) => {
            // User specified RUST_LOG: use it, but suppress ORT logs unless explicitly set to include them
            if !log_env.contains("ort=") {
                format!("{},ort=off", log_env)
            } else {
                log_env
            }
        }
        Err(_) => {
            // Default: suppress ORT logs, show info level for app
            "info,ort=off,ort::logging=off".to_string()
        }
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

/// Emit an error. In JSON mode: `{"error": "..."}` on stdout. Otherwise: `Error: ...` on stderr.
pub fn emit_error(msg: &str, json: bool) {
    if json {
        println!("{}", serde_json::json!({ "error": msg }));
    } else {
        eprintln!("Error: {msg}");
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
            let config = Config::load();
            return commands::info::run(args.clone(), &config, cli.db.as_deref(), cli.json);
        }
        Commands::Init(args) => {
            return commands::init::run(args.clone()).await;
        }
        Commands::Migrate(args) => {
            let config = Config::load();
            return commands::migrate::run(args.clone(), &config, cli.db.as_deref(), cli.json).await;
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

    // Load config
    let config = Config::load();
    
    // Route to appropriate backend - independent paths, no pool mixing
    match config.database.backend.as_str() {
        "neo4j" => {
            // Neo4j backend - no SQLite pool
            let neo4j_config = config.database.neo4j.as_ref()
                .ok_or_else(|| anyhow::anyhow!("Neo4j config missing in config.toml"))?;
            
            let db = Arc::new(voidm_neo4j::Neo4jDatabase::connect(
                &neo4j_config.uri,
                &neo4j_config.username,
                &neo4j_config.password,
                &neo4j_config.database,
            ).await?) as Arc<dyn voidm_db_trait::Database>;
            
            db.ensure_schema().await?;

            // Dummy pool for command compatibility (Neo4j doesn't use it)
            let dummy_pool = open_pool(std::path::Path::new(":memory:")).await?;

            match cli.command {
                Commands::Add(args) => commands::add::run(args, &db, &dummy_pool, &config, cli.json).await,
                Commands::Get(args) => commands::get::run(args, &db, &dummy_pool, cli.json).await,
                Commands::Search(args) => commands::search::run(args, &db, &dummy_pool, &config, cli.json).await,
                Commands::List(args) => commands::list::run(args, &db, &dummy_pool, &config, cli.json).await,
                Commands::Delete(args) => commands::delete::run(args, &db, &dummy_pool, cli.json).await,
                Commands::Link(args) => commands::link::run(args, &db, &dummy_pool, cli.json).await,
                Commands::Unlink(args) => commands::unlink::run(args, &db, &dummy_pool, cli.json).await,
                Commands::Graph(cmd) => commands::graph::run(cmd, &db, &dummy_pool, cli.json).await,
                Commands::Ontology(cmd) => commands::ontology::run(cmd, &db, &dummy_pool, &config, cli.json).await,
                Commands::Conflicts(cmd) => commands::conflicts::run(cmd, &db, &dummy_pool, cli.json).await,
                Commands::Scopes(cmd) => commands::scopes::run(cmd, &db, &dummy_pool, cli.json).await,
                Commands::Export(args) => commands::export::run(args, &db, &dummy_pool, &config, cli.json).await,
                Commands::Config(_) => unreachable!(),
                Commands::Models(cmd) => commands::models::run(cmd, &db, &dummy_pool, &config, cli.json).await,
                Commands::Instructions(_) => unreachable!(),
                Commands::Info(_) => unreachable!(),
                Commands::Init(_) => unreachable!(),
                Commands::Migrate(_) => unreachable!(),
                Commands::CheckUpdate(_) => unreachable!(),
                Commands::Consolidate(args) => commands::consolidate::run(args, &db, &dummy_pool, &config, cli.json).await,
                Commands::Validate(args) => commands::validate::run(args, &db).await,
                Commands::Chunk(args) => commands::chunk::run(args, &db).await,
                Commands::Stats(args) => commands::stats::run(args, &db, &dummy_pool, &config, cli.json).await,
                Commands::Mcp(_) => anyhow::bail!("MCP server is only available with SQLite backend"),
            }
        }
        "sqlite" | _ => {
            // SQLite backend - with pool
            let db_path = config.db_path(cli.db.as_deref());
            let pool = open_pool(&db_path).await?;
            let db = Arc::new(voidm_sqlite::SqliteDatabase::new(pool.clone())) as Arc<dyn voidm_db_trait::Database>;

            // Migrations already run in open_pool() via ensure_schema()
            let _ = voidm_core::vector::cleanup_stale_temp_table(&pool).await;

            // Check model mismatch
            if config.embeddings.enabled {
                if let Ok(Some((db_model, db_dim))) =
                    voidm_core::crud_trait::check_model_mismatch(&db, &config.embeddings.model).await
                {
                    eprintln!(
                        "Warning: configured model '{}' differs from DB model '{}' (dim {}). \
                         Vector search disabled. Run 'voidm models reembed' to re-embed all memories.",
                        config.embeddings.model, db_model, db_dim
                    );
                }
            }

            let result = match cli.command {
                Commands::Add(args) => commands::add::run(args, &db, &pool, &config, cli.json).await,
                Commands::Get(args) => commands::get::run(args, &db, &pool, cli.json).await,
                Commands::Search(args) => commands::search::run(args, &db, &pool, &config, cli.json).await,
                Commands::List(args) => commands::list::run(args, &db, &pool, &config, cli.json).await,
                Commands::Delete(args) => commands::delete::run(args, &db, &pool, cli.json).await,
                Commands::Link(args) => commands::link::run(args, &db, &pool, cli.json).await,
                Commands::Unlink(args) => commands::unlink::run(args, &db, &pool, cli.json).await,
                Commands::Graph(cmd) => commands::graph::run(cmd, &db, &pool, cli.json).await,
                Commands::Ontology(cmd) => commands::ontology::run(cmd, &db, &pool, &config, cli.json).await,
                Commands::Conflicts(cmd) => commands::conflicts::run(cmd, &db, &pool, cli.json).await,
                Commands::Scopes(cmd) => commands::scopes::run(cmd, &db, &pool, cli.json).await,
                Commands::Export(args) => commands::export::run(args, &db, &pool, &config, cli.json).await,
                Commands::Config(_) => unreachable!(),
                Commands::Models(cmd) => commands::models::run(cmd, &db, &pool, &config, cli.json).await,
                Commands::Instructions(_) => unreachable!(),
                Commands::Info(_) => unreachable!(),
                Commands::Init(_) => unreachable!(),
                Commands::Migrate(_) => unreachable!(),
                Commands::CheckUpdate(_) => unreachable!(),
                Commands::Consolidate(args) => commands::consolidate::run(args, &db, &pool, &config, cli.json).await,
                Commands::Validate(args) => commands::validate::run(args, &db).await,
                Commands::Chunk(args) => commands::chunk::run(args, &db).await,
                Commands::Stats(args) => commands::stats::run(args, &db, &pool, &config, cli.json).await,
                Commands::Mcp(args) => commands::mcp::run(args, pool.clone(), config).await,
            };

            // Perform backend-specific shutdown (SQLite: WAL checkpoint, etc.)
            db.shutdown().await?;
            pool.close().await;
            
            result
        }
    }
}
