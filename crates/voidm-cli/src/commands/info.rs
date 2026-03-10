use anyhow::Result;
use clap::Args;
use voidm_core::Config;

#[derive(Args, Clone)]
pub struct InfoArgs {}

pub fn run(_args: InfoArgs, config: &Config, db_override: Option<&str>, json: bool) -> Result<()> {
    let db_path = config.db_path(db_override);
    let db_exists = db_path.exists();
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).ok();

    let config_path = voidm_core::config_path_display();

    let active_source = if db_override.is_some() {
        "--db flag"
    } else if std::env::var("VOIDM_DB").map(|v| !v.is_empty()).unwrap_or(false) {
        "$VOIDM_DB"
    } else if !config.database.sqlite_path.is_empty() {
        "config file"
    } else if config.database.path.as_ref().map(|p| !p.is_empty()).unwrap_or(false) {
        "config file (legacy path field)"
    } else {
        "default (XDG)"
    };

    let embedding_model = &config.embeddings.model;
    let embeddings_enabled = config.embeddings.enabled;

    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "db": {
                "path": db_path.display().to_string(),
                "exists": db_exists,
                "size_bytes": db_size,
                "source": active_source,
            },
            "config": {
                "path": config_path,
            },
            "embeddings": {
                "enabled": embeddings_enabled,
                "model": embedding_model,
            },
            "search": {
                "default_mode": config.search.mode,
                "min_score": (config.search.min_score as f64 * 100.0).round() / 100.0,
                "default_limit": config.search.default_limit,
            }
        }))?);
    } else {
        println!("Database");
        println!("  Path:    {}", db_path.display());
        println!("  Exists:  {}", if db_exists { "yes" } else { "no (will be created on first write)" });
        if let Some(sz) = db_size {
            println!("  Size:    {}", human_size(sz));
        }
        println!("  Source:  {}", active_source);
        println!();
        println!("Config");
        println!("  Path:    {}", config_path);
        println!();
        println!("Embeddings");
        println!("  Enabled: {}", embeddings_enabled);
        println!("  Model:   {}", embedding_model);
        println!();
        println!("Search defaults");
        println!("  Mode:      {}", config.search.mode);
        println!("  Min score: {} (hybrid only)", config.search.min_score);
        println!("  Limit:     {}", config.search.default_limit);
    }
    Ok(())
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0) }
}
