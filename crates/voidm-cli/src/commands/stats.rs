use anyhow::Result;
use clap::Args;
use voidm_core::Config;

#[derive(Args)]
pub struct StatsArgs {}

pub async fn run(_args: StatsArgs, db: &std::sync::Arc<dyn voidm_db::Database>, config: &Config, json: bool) -> Result<()> {
    // Get all statistics from database trait method (no sqlx here!)
    let mut stats = db.get_statistics().await?;

    // Add DB file size only for SQLite backend. Neo4j is not path-backed here.
    if config.database.backend == "sqlite" {
        let db_path = config.db_path(None);
        stats.db_size_bytes = std::fs::metadata(&db_path)
            .map(|m| m.len())
            .unwrap_or(0);
    } else {
        stats.db_size_bytes = 0;
    }

    if json {
        let mut type_map = serde_json::Map::new();
        for (t, c) in &stats.memories_by_type {
            type_map.insert(t.clone(), serde_json::json!(c));
        }
        let edge_map: serde_json::Map<String, serde_json::Value> = stats.graph.edges_by_type.iter()
            .map(|(t, c)| (t.clone(), serde_json::json!(c)))
            .collect();

        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "result": {
                "memories": {
                    "total": stats.total_memories,
                    "by_type": type_map,
                    "embedded": stats.embedding_coverage.total_embeddings,
                    "embedding_coverage_pct": stats.embedding_coverage.coverage_percentage.round() as i64
                },
                "scopes": stats.scopes_count,
                "tags": stats.top_tags.iter().map(|(t, c)| serde_json::json!({"tag": t, "count": c})).collect::<Vec<_>>(),
                "graph": {
                    "nodes": stats.graph.node_count,
                    "edges": stats.graph.edge_count,
                    "by_rel_type": edge_map
                },
                "db_size_bytes": stats.db_size_bytes
            }
        }))?);
    } else {
        println!("Memories:  {} total", stats.total_memories);
        for (t, c) in &stats.memories_by_type {
            println!("  {:12} {}", t, c);
        }
        if stats.embedding_coverage.total_embeddings < stats.total_memories {
            println!("  Embedded:  {}/{} ({:.0}%)",
                stats.embedding_coverage.total_embeddings, stats.total_memories, 
                stats.embedding_coverage.coverage_percentage);
        } else if stats.total_memories > 0 {
            println!("  Embedded:  {}/{} (100%)", stats.embedding_coverage.total_embeddings, stats.total_memories);
        }
        println!("Scopes:    {}", stats.scopes_count);
        if !stats.top_tags.is_empty() {
            let tag_str: Vec<String> = stats.top_tags.iter()
                .map(|(t, c)| format!("{}({})", t, c))
                .collect();
            println!("Top tags:  {}", tag_str.join(", "));
        }
        println!("Graph:     {} nodes, {} edges", stats.graph.node_count, stats.graph.edge_count);
        for (rel, cnt) in &stats.graph.edges_by_type {
            println!("  {:20} {}", rel, cnt);
        }
        println!("DB size:   {}", human_size(stats.db_size_bytes));
    }
    Ok(())
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0) }
}
