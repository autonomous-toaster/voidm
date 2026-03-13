use anyhow::Result;
use clap::Args;
use sqlx::SqlitePool;
use std::time::Instant;
use voidm_core::{Config, crud, user_interactions::track_interaction};

#[derive(Args)]
pub struct ExportArgs {
    /// Filter by scope prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Export format: json, markdown
    #[arg(long, default_value = "json")]
    pub format: String,

    /// Output file (default: stdout)
    #[arg(long, short = 'o')]
    pub output: Option<String>,

    #[arg(long, default_value = "1000")]
    pub limit: usize,
}

pub async fn run(args: ExportArgs, pool: &SqlitePool, _config: &Config, _json: bool) -> Result<()> {
    let start = Instant::now();
    let user_id = std::env::var("VOIDM_USER").unwrap_or_else(|_| "agent-cli".to_string());
    
    let memories = crud::list_memories(pool, args.scope.as_deref(), None, args.limit).await?;

    let content = match args.format.as_str() {
        "json" => serde_json::to_string_pretty(&memories)?,
        "markdown" => {
            let mut md = String::new();
            md.push_str("# voidm Memory Export\n\n");
            for m in &memories {
                md.push_str(&format!("## {} [{}]\n\n", m.id, m.memory_type));
                md.push_str(&format!("- **Importance**: {}\n", m.importance));
                md.push_str(&format!("- **Created**: {}\n", m.created_at));
                if !m.scopes.is_empty() {
                    md.push_str(&format!("- **Scopes**: {}\n", m.scopes.join(", ")));
                }
                if !m.tags.is_empty() {
                    md.push_str(&format!("- **Tags**: {}\n", m.tags.join(", ")));
                }
                md.push('\n');
                md.push_str(&m.content);
                md.push_str("\n\n---\n\n");
            }
            md
        }
        other => anyhow::bail!("Unknown export format: '{}'. Valid: json, markdown", other),
    };

    // Track the export interaction
    let duration_ms = start.elapsed().as_millis() as i64;
    let context = format!("export:{}", args.format);
    let _ = track_interaction(
        pool,
        &user_id,
        "export",
        "bulk_export",
        &format!("{} memories", memories.len()),
        "success",
        duration_ms,
        Some(&context),
    ).await;

    if let Some(path) = args.output {
        std::fs::write(&path, &content)?;
        eprintln!("Exported {} memories to {}", memories.len(), path);
    } else {
        print!("{}", content);
    }
    Ok(())
}
