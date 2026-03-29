use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};
use voidm_core::{Config, crud_trait, models::MemoryEdge};
use std::sync::Arc;

#[derive(Args)]
pub struct ExportArgs {
    /// Filter by scope prefix
    #[arg(long)]
    pub scope: Option<String>,

    /// Export format: json, markdown, full (json with all relationships)
    #[arg(long, default_value = "json")]
    pub format: String,

    /// Output file (default: stdout)
    #[arg(long, short = 'o')]
    pub output: Option<String>,

    #[arg(long, default_value = "1000")]
    pub limit: usize,

    /// Include all relationships
    #[arg(long)]
    pub with_edges: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportData {
    pub memories: Vec<voidm_core::models::Memory>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<MemoryEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ExportMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportMetadata {
    pub total_memories: usize,
    pub total_edges: usize,
    pub exported_at: String,
    pub scopes_included: Vec<String>,
}

pub async fn run(args: ExportArgs, db: &std::sync::Arc<dyn voidm_db_trait::Database>, _config: &Config, _json: bool) -> Result<()> {
    
    let memories = crud_trait::list_memories_filtered(db, args.scope.as_deref(), None, Some(args.limit)).await?;
    let mut edges = Vec::new();

    // Get edges if requested or format is "full"
    if args.with_edges || args.format == "full" {
        let edges_json = db.list_edges().await.unwrap_or_default();
        for edge_json in edges_json {
            if let Ok(edge) = serde_json::from_value::<voidm_core::models::MemoryEdge>(edge_json) {
                edges.push(edge);
            }
        }
    }

    let content = match args.format.as_str() {
        "json" | "full" => {
            let scopes: Vec<String> = memories.iter()
                .flat_map(|m| m.scopes.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            let export_data = ExportData {
                memories: memories.clone(),
                edges: edges.clone(),
                metadata: Some(ExportMetadata {
                    total_memories: memories.len(),
                    total_edges: edges.len(),
                    exported_at: chrono::Utc::now().to_rfc3339(),
                    scopes_included: scopes,
                }),
            };
            serde_json::to_string_pretty(&export_data)?
        }
        "markdown" => {
            let mut md = String::new();
            md.push_str("# voidm Memory Export\n\n");
            
            // Add metadata
            if !args.format.is_empty() {
                md.push_str(&format!("**Exported**: {}\n", chrono::Utc::now().to_rfc3339()));
                md.push_str(&format!("**Memories**: {}\n", memories.len()));
                if args.with_edges || args.format == "full" {
                    md.push_str(&format!("**Edges**: {}\n", edges.len()));
                }
                md.push_str("\n---\n\n");
            }

            // Export memories
            md.push_str("## Memories\n\n");
            for m in &memories {
                md.push_str(&format!("### {} [{}]\n\n", m.id, m.memory_type));
                md.push_str(&format!("- **Importance**: {}\n", m.importance));
                md.push_str(&format!("- **Quality**: {}\n", m.quality_score.unwrap_or(0.0)));
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

            // Export edges if included
            if (args.with_edges || args.format == "full") && !edges.is_empty() {
                md.push_str("## Memory Relationships\n\n");
                for edge in &edges {
                    md.push_str(&format!("- `{}` **[{}]** → `{}`", edge.from_id, edge.rel_type, edge.to_id));
                    if let Some(note) = &edge.note {
                        md.push_str(&format!(" ({})", note));
                    }
                    md.push('\n');
                }
                md.push_str("\n");
            }

            md
        }
        other => anyhow::bail!("Unknown export format: '{}'. Valid: json, markdown, full", other),
    };

    if let Some(path) = args.output {
        std::fs::write(&path, &content)?;
        let msg = if args.with_edges {
            format!("Exported {} memories + {} edges to {}", 
                memories.len(), edges.len(), path)
        } else {
            format!("Exported {} memories to {}", memories.len(), path)
        };
        eprintln!("{}", msg);
    } else {
        print!("{}", content);
    }
    Ok(())
}
