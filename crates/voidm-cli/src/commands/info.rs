use anyhow::Result;
use clap::Args;
use voidm_core::Config;

#[derive(Args, Clone)]
pub struct InfoArgs {}

pub fn run(_args: InfoArgs, config: &Config, db_override: Option<&str>, json: bool) -> Result<()> {
    let config_path = voidm_core::config_path_display();
    let backend = config.database.backend.clone();

    let sqlite_info = if backend == "sqlite" {
        let db_path = config.db_path(db_override);
        let db_exists = db_path.exists();
        let db_size = std::fs::metadata(&db_path).map(|m| m.len()).ok();
        let active_source = if db_override.is_some() {
            "--db flag (SQLite only)"
        } else if std::env::var("VOIDM_DB").map(|v| !v.is_empty()).unwrap_or(false) {
            "$VOIDM_DB (SQLite only)"
        } else if config.database.sqlite.as_ref().and_then(|s| s.path.as_ref()).map(|p| !p.is_empty()).unwrap_or(false) {
            "config file [database.sqlite].path"
        } else if !config.database.sqlite_path.is_empty() {
            "config file legacy database.sqlite_path"
        } else if config.database.path.as_ref().map(|p| !p.is_empty()).unwrap_or(false) {
            "config file legacy database.path"
        } else {
            "default (XDG)"
        };
        Some((db_path, db_exists, db_size, active_source))
    } else {
        None
    };

    let embedding_model = &config.embeddings.model;
    let embeddings_enabled = config.embeddings.enabled;

    if json {
        crate::output::print_result(&serde_json::json!({
            "database": if let Some((db_path, db_exists, db_size, active_source)) = &sqlite_info {
                serde_json::json!({
                    "backend": backend,
                    "path": db_path.display().to_string(),
                    "exists": db_exists,
                    "size_bytes": db_size,
                    "source": active_source
                })
            } else {
                serde_json::json!({
                    "backend": backend,
                    "neo4j": config.database.neo4j.as_ref().map(|neo4j| serde_json::json!({
                        "uri": neo4j.uri,
                        "username": neo4j.username,
                        "database": neo4j.database,
                        "password": "[REDACTED]"
                    }))
                })
            },
            "config": {
                "path": config_path
            },
            "embeddings": {
                "enabled": embeddings_enabled,
                "model": embedding_model
            },
            "search": {
                "default_mode": config.search.mode,
                "min_score": (config.search.min_score as f64 * 100.0).round() / 100.0,
                "default_limit": config.search.default_limit,
                "query_expansion_backend": config.search.query_expansion.as_ref().map(|qe| qe.backend.clone())
            },
            "enrichment": {
                "auto_tagging_backend": config.enrichment.auto_tagging.backend
            }
        }))?;
    } else {
        println!("Database");
        println!("  Backend: {}", backend);
        if let Some((db_path, db_exists, db_size, active_source)) = &sqlite_info {
            println!("  Path:    {}", db_path.display());
            println!("  Exists:  {}", if *db_exists { "yes" } else { "no (will be created on first write)" });
            if let Some(sz) = db_size {
                println!("  Size:    {}", human_size(*sz));
            }
            println!("  Source:  {}", active_source);
        } else if let Some(neo4j) = &config.database.neo4j {
            println!("  URI:     {}", neo4j.uri);
            println!("  User:    {}", neo4j.username);
            println!("  DB:      {}", neo4j.database);
            println!("  Source:  config file [database.neo4j]");
        }
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
        if let Some(qe) = &config.search.query_expansion {
            println!("  QE backend: {:?}", qe.backend);
        }
        println!();
        println!("Enrichment");
        println!("  Auto-tag backend: {}", config.enrichment.auto_tagging.backend);
    }
    Ok(())
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0) }
}
