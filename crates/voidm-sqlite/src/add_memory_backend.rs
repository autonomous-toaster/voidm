//! Transaction execution for add_memory
//! This module handles all database operations for memory creation.

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use voidm_core::models::{AddMemoryRequest, AddMemoryResponse, EdgeType};
use voidm_scoring::QualityScore;
use serde_json::json;
use chrono::DateTime;

/// Wrapper that orchestrates the full add_memory flow:
/// 1. Pre-transaction logic (validation, embeddings, scoring)
/// 2. Transaction execution
/// 3. Post-transaction logic (auto-extract, auto-link)
pub async fn execute_add_memory_transaction_wrapper(
    pool: &SqlitePool,
    req: AddMemoryRequest,
    config: &voidm_core::Config,
) -> Result<AddMemoryResponse> {
    // Delegate to the pool-based voidm-core implementation
    // This handles all pre-tx, tx, and post-tx logic
    voidm_core::crud::add_memory(pool, req, config).await
}

/// Execute the atomic transaction for adding a memory
pub async fn execute_add_memory_transaction(
    pool: &SqlitePool,
    id: &str,
    memory_type_str: &str,
    req: &AddMemoryRequest,
    embedding_result: Option<Vec<f32>>,
    quality: QualityScore,
    tags_json: &str,
    metadata_json: &str,
    resolved_link_targets: Vec<(EdgeType, Option<String>, String)>,
    now: &str,
) -> Result<AddMemoryResponse> {
    // Begin transaction
    let mut tx = pool.begin().await?;

    // Insert memory with persistent quality_score and context
    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, quality_score, context, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(id)
    .bind(memory_type_str)
    .bind(&req.content)
    .bind(req.importance)
    .bind(tags_json)
    .bind(metadata_json)
    .bind(quality.score)
    .bind(&req.context)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .context("Failed to insert memory")?;

    // Insert scopes
    for scope in &req.scopes {
        sqlx::query("INSERT OR IGNORE INTO memory_scopes (memory_id, scope) VALUES (?, ?)")
            .bind(id)
            .bind(scope)
            .execute(&mut *tx)
            .await?;
    }

    // Insert FTS
    sqlx::query("INSERT INTO memories_fts (id, content) VALUES (?, ?)")
        .bind(id)
        .bind(&req.content)
        .execute(&mut *tx)
        .await?;

    // Insert embedding
    if let Some(ref emb) = embedding_result {
        let bytes: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
        sqlx::query(
            "INSERT INTO vec_memories (memory_id, embedding) VALUES (?, ?)"
        )
        .bind(id)
        .bind(&bytes)
        .execute(&mut *tx)
        .await
        .context("Failed to insert into vec_memories")?;
    }

    // Graph node upsert
    sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let node_id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO graph_node_labels (node_id, label) VALUES (?, 'Memory')")
        .bind(node_id)
        .execute(&mut *tx)
        .await?;

    // Store memory_type as a text property on the graph node
    let key_id = intern_property_key_in_tx(&mut tx, "memory_type").await?;
    sqlx::query(
        "INSERT OR REPLACE INTO graph_node_props_text (node_id, key_id, value) VALUES (?, ?, ?)"
    )
    .bind(node_id)
    .bind(key_id)
    .bind(memory_type_str)
    .execute(&mut *tx)
    .await?;

    // Create --link edges
    for (edge_type, note, target_id) in resolved_link_targets {
        let target_node: i64 = sqlx::query_scalar(
            "SELECT n.id FROM graph_nodes n WHERE n.memory_id = ?"
        )
        .bind(&target_id)
        .fetch_one(&mut *tx)
        .await
        .with_context(|| format!("Graph node not found for target '{}'", target_id))?;

        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(node_id)
        .bind(target_node)
        .bind(edge_type.as_str())
        .bind(&note)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await.context("Transaction commit failed")?;

    // Build response
    let scopes: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM memory_scopes WHERE memory_id = ? ORDER BY scope"
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
    let metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap_or(json!({}));

    Ok(AddMemoryResponse {
        id: id.to_string(),
        memory_type: memory_type_str.to_string(),
        content: req.content.clone(),
        scopes,
        tags,
        importance: req.importance,
        created_at: now.to_string(),
        quality_score: Some(quality.score),
        metadata,
        suggested_links: Vec::new(),
        duplicate_warning: None,
        context: req.context.clone(),
        title: req.title.clone(),
    })
}

/// Transaction-scoped property key interning
async fn intern_property_key_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &str,
) -> Result<i64> {
    sqlx::query("INSERT OR IGNORE INTO graph_property_keys (key) VALUES (?)")
        .bind(key)
        .execute(&mut **tx)
        .await?;
    let id: i64 = sqlx::query_scalar("SELECT id FROM graph_property_keys WHERE key = ?")
        .bind(key)
        .fetch_one(&mut **tx)
        .await?;
    Ok(id)
}
