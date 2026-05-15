use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use voidm_db::Database;

#[derive(Args)]
pub struct RepairArgs {
    /// Only delete orphaned chunks (fast, no model loading)
    #[arg(long, default_value_t = false)]
    pub orphans_only: bool,
}

pub async fn run(_args: RepairArgs, db: &Arc<dyn Database>, json: bool) -> Result<()> {
    let mut fixes = Vec::<String>::new();

    // 1. Delete orphaned chunks (always runs — fast, improves data quality)
    let orphan_cypher = "MATCH (c:MemoryChunk) WHERE NOT ()-[:HAS_CHUNK]->(c) WITH c LIMIT 1000 DELETE c RETURN count(c) as removed";
    let orphan_result = db.query_cypher(orphan_cypher, &serde_json::json!({})).await;
    let mut orphans_removed = 0usize;
    match orphan_result {
        Ok(val) => {
            if let Some(rows) = val.as_array() {
                if let Some(first) = rows.first() {
                    if let Some(obj) = first.as_object() {
                        if let Some(serde_json::Value::Number(n)) = obj.get("removed") {
                            orphans_removed = n.as_u64().unwrap_or(0) as usize;
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Repair: orphan cleanup failed: {}", e);
        }
    }

    if orphans_removed > 0 {
        fixes.push(format!("Removed {} orphaned chunk(s)", orphans_removed));
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "result": {
                    "orphans_removed": orphans_removed,
                    "fixes": fixes,
                }
            }))?
        );
    } else {
        if fixes.is_empty() {
            println!("No issues found. System is clean.");
        } else {
            println!("Repairs applied:");
            for fix in &fixes {
                println!("  ✓ {}", fix);
            }
        }
    }

    Ok(())
}
