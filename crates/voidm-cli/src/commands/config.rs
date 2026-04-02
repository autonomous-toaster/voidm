use anyhow::Result;
use clap::Subcommand;
use voidm_core::{Config, config::{save_config, save_config_template}};

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current config
    Show,
    /// Create an initial config file from defaults
    Init {
        /// Overwrite an existing config file
        #[arg(long)]
        force: bool,
    },
    /// Set a config value (key=value dot-notation)
    Set {
        /// Config key (e.g. embeddings.model)
        key: String,
        /// New value
        value: String,
    },
}

pub async fn run(cmd: &ConfigCommands, json: bool) -> Result<()> {
    match cmd {
        ConfigCommands::Show => {
            let config = Config::load();
            let mut redacted = serde_json::to_value(&config)?;
            crate::output::redact_secret_values(&mut redacted);
            if json {
                crate::output::print_result(&redacted)?;
            } else {
                println!("{}", serde_json::to_string_pretty(&redacted)?);
            }
        }
        ConfigCommands::Init { force } => {
            let path = voidm_core::config::config_path_for_write()?;
            let existed = path.exists();
            if existed && !force {
                anyhow::bail!(
                    "Config file already exists at {}. Use 'voidm config init --force' to overwrite it.",
                    path.display()
                );
            }
            let config = Config::default();
            save_config_template(&config)?;
            if json {
                crate::output::print_result(&serde_json::json!({
                    "initialized": true,
                    "path": path.display().to_string(),
                    "overwritten": existed && *force,
                }))?;
            } else {
                println!("Initialized config: {}", path.display());
            }
        }
        ConfigCommands::Set { key, value } => {
            let mut config = Config::load();
            apply_config_key(&mut config, key, value)?;
            save_config(&config)?;
            if json {
                crate::output::print_result(&serde_json::json!({
                    "updated": true,
                    "key": key,
                    "value": value,
                }))?;
            } else {
                eprintln!("Set {} = {}", key, value);
            }
        }
    }
    Ok(())
}

fn apply_config_key(config: &mut Config, key: &str, value: &str) -> Result<()> {
    match key {
        "embeddings.model" => config.embeddings.model = value.to_string(),
        "embeddings.enabled" => config.embeddings.enabled = value.parse()?,
        "search.mode" => config.search.mode = value.to_string(),
        "search.default_limit" => config.search.default_limit = value.parse()?,
        "search.min_score" => config.search.min_score = value.parse()?,
        "search.query_expansion.backend" => {
            if value == "onnx" || value == "llama_cpp" || value == "mlx" {
                #[cfg(feature = "query-expansion")]
                {
                    let qe = config.search.query_expansion.get_or_insert_with(Default::default);
                    qe.backend = voidm_core::query_expansion::parse_generation_backend(value)?;
                }
                #[cfg(not(feature = "query-expansion"))]
                anyhow::bail!("query-expansion feature is not enabled in this build");
            } else {
                anyhow::bail!("Invalid search.query_expansion.backend '{}'. Valid: onnx, llama_cpp, mlx", value)
            }
        }
        "enrichment.auto_tagging.backend" => {
            if value == "onnx" || value == "llama_cpp" || value == "mlx" {
                config.enrichment.auto_tagging.backend = value.to_string();
            } else {
                anyhow::bail!("Invalid enrichment.auto_tagging.backend '{}'. Valid: onnx, llama_cpp, mlx", value)
            }
        },
        "insert.auto_link_threshold" => config.insert.auto_link_threshold = value.parse()?,
        "insert.duplicate_threshold" => config.insert.duplicate_threshold = value.parse()?,
        "insert.auto_link_limit" => config.insert.auto_link_limit = value.parse()?,
        "database.backend" => config.database.backend = value.to_string(),
        "database.sqlite_path" => config.database.sqlite_path = value.to_string(),
        "database.path" => config.database.path = Some(value.to_string()), // legacy
        other => anyhow::bail!("Unknown config key: '{}'. Valid keys: embeddings.model, embeddings.enabled, search.mode, search.default_limit, search.min_score, search.query_expansion.backend, enrichment.auto_tagging.backend, insert.auto_link_threshold, insert.duplicate_threshold, insert.auto_link_limit, database.backend, database.sqlite_path, database.path", other),
    }
    Ok(())
}
