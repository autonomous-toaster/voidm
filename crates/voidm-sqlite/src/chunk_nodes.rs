//! Store chunks as generic nodes with ordering (Phase 0.5.3)
//!
//! Converts text chunks into generic node/edge format with:
//! - Chunk node: type="Chunk", properties={sequence_num, char_start, char_end, content}
//! - Edge: Memory -[:HAS_CHUNK]-> Chunk with properties={sequence_num}

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::SqlitePool;

/// Compute character positions for chunks with proper byte offsets
///
/// Returns: Vec of (sequence_num, char_start, char_end, content)
pub fn compute_chunk_positions(text: &str, chunks: &[String]) -> Vec<(usize, usize, usize, String)> {
    let mut positions = Vec::new();
    let mut current_pos = 0;

    for (seq_num, chunk) in chunks.iter().enumerate() {
        // Find this chunk in remaining text (accounting for overlaps)
        if let Some(chunk_start) = text[current_pos..].find(chunk) {
            let absolute_start = current_pos + chunk_start;
            let absolute_end = absolute_start + chunk.len();

            positions.push((
                seq_num,
                absolute_start,
                absolute_end,
                chunk.clone(),
            ));

            // Move position forward (but keep configured default overlap region for next search)
            current_pos = absolute_end.saturating_sub(voidm_core::embeddings::DEFAULT_OVERLAP);
        } else {
            // Fallback: just add sequential positions
            let start = current_pos;
            let end = current_pos + chunk.len();
            positions.push((seq_num, start, end, chunk.clone()));
            current_pos = end;
        }
    }

    positions
}

/// Store chunks as generic nodes and create edges to memory
pub async fn store_chunks_as_nodes(
    pool: &SqlitePool,
    memory_id: &str,
    chunks: &[String],
) -> Result<()> {
    let positions = compute_chunk_positions(&chunks.join("\n"), chunks);

    for (seq_num, char_start, char_end, content) in positions {
        // Generate deterministic chunk ID
        let chunk_id = format!("chunk:{}:#{}", memory_id, seq_num);

        // Create chunk node with ordering fields
        let properties = json!({
            "sequence_num": seq_num,
            "char_start": char_start,
            "char_end": char_end,
            "content": content,
            "memory_id": memory_id
        });

        // Insert chunk node (using generic insert)
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&chunk_id)
        .bind("MemoryChunk")
        .bind(properties.to_string())
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .context("Failed to insert chunk node")?;

        // Create edge: Memory -[:HAS_CHUNK]-> Chunk
        let edge_id = format!("{}:HAS_CHUNK:{}", memory_id, chunk_id);
        let edge_properties = json!({
            "sequence_num": seq_num
        });

        sqlx::query(
            "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&edge_id)
        .bind(memory_id)
        .bind("HAS_CHUNK")
        .bind(&chunk_id)
        .bind(edge_properties.to_string())
        .bind(&now)
        .execute(pool)
        .await
        .context("Failed to create HAS_CHUNK edge")?;
    }

    Ok(())
}

/// Reconstruct full memory from ordered chunks
pub async fn reconstruct_from_chunks(
    pool: &SqlitePool,
    memory_id: &str,
) -> Result<String> {
    let rows = sqlx::query_as::<_, (String,)>(
        "SELECT to_id FROM edges WHERE from_id = ? AND edge_type = 'HAS_CHUNK' ORDER BY json_extract(properties, '$.sequence_num') ASC"
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch chunk edges")?;

    let mut chunks = Vec::new();

    for (to_id,) in rows {
        if let Ok(Some((node_props,))) = sqlx::query_as::<_, (String,)>(
            "SELECT properties FROM nodes WHERE id = ? AND type = 'MemoryChunk'"
        )
        .bind(&to_id)
        .fetch_optional(pool)
        .await {
            if let Ok(node) = serde_json::from_str::<Value>(&node_props) {
                if let Some(content) = node.get("content").and_then(|v| v.as_str()) {
                    chunks.push(content.to_string());
                }
            }
        }
    }

    // Join with minimal overlap removal (next phase can add de-duplication)
    Ok(chunks.join("\n"))
}
