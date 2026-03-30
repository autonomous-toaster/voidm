//! Transaction execution for add_memory
//! This module handles all database operations for memory creation.

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use voidm_db::models::{AddMemoryRequest, AddMemoryResponse, EdgeType};
use voidm_scoring::QualityScore;
use serde_json::json;

/// Data prepared before transaction begins
/// Separates pre-transaction logic (in voidm-core) from transaction execution (backend-only)
pub struct PreTxData {
    pub id: String,
    pub memory_type_str: String,
    pub content: String,
    pub importance: i64,
    pub tags_json: String,
    pub metadata_json: String,
    pub context: Option<String>,
    pub scopes: Vec<String>,
    pub quality: QualityScore,
    pub embedding_result: Option<Vec<f32>>,
    pub resolved_link_targets: Vec<(EdgeType, Option<String>, String)>,
    pub now: String,
    pub title: Option<String>,
}

/// Wrapper that executes only the transaction part
/// All pre-transaction logic (validation, embeddings, scoring, ID resolution) happens in voidm-core
pub async fn execute_add_memory_transaction_wrapper(
    pool: &SqlitePool,
    pre_tx_data: PreTxData,
) -> Result<AddMemoryResponse> {
    // Execute the atomic transaction
    execute_add_memory_transaction(pool, pre_tx_data).await
}

/// Execute the atomic transaction for adding a memory
pub async fn execute_add_memory_transaction(
    pool: &SqlitePool,
    pre_tx: PreTxData,
) -> Result<AddMemoryResponse> {
    // Begin transaction
    let mut tx = pool.begin().await?;

    // Insert memory with persistent quality_score and context
    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, quality_score, context, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&pre_tx.id)
    .bind(&pre_tx.memory_type_str)
    .bind(&pre_tx.content)
    .bind(pre_tx.importance)
    .bind(&pre_tx.tags_json)
    .bind(&pre_tx.metadata_json)
    .bind(pre_tx.quality.score)
    .bind(&pre_tx.context)
    .bind(&pre_tx.now)
    .bind(&pre_tx.now)
    .execute(&mut *tx)
    .await
    .context("Failed to insert memory")?;

    // Insert scopes
    for scope in &pre_tx.scopes {
        sqlx::query("INSERT OR IGNORE INTO memory_scopes (memory_id, scope) VALUES (?, ?)")
            .bind(&pre_tx.id)
            .bind(scope)
            .execute(&mut *tx)
            .await?;
    }

    // Insert FTS
    sqlx::query("INSERT INTO memories_fts (id, content) VALUES (?, ?)")
        .bind(&pre_tx.id)
        .bind(&pre_tx.content)
        .execute(&mut *tx)
        .await?;

    // Insert embedding
    if let Some(ref emb) = pre_tx.embedding_result {
        let bytes: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
        sqlx::query(
            "INSERT INTO vec_memories (memory_id, embedding) VALUES (?, ?)"
        )
        .bind(&pre_tx.id)
        .bind(&bytes)
        .execute(&mut *tx)
        .await
        .context("Failed to insert into vec_memories")?;
    }

    // Graph node upsert
    sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
        .bind(&pre_tx.id)
        .execute(&mut *tx)
        .await?;
    let node_id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(&pre_tx.id)
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
    .bind(&pre_tx.memory_type_str)
    .execute(&mut *tx)
    .await?;

    // Create --link edges
    for (edge_type, note, target_id) in &pre_tx.resolved_link_targets {
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
        .bind(&pre_tx.now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await.context("Transaction commit failed")?;

    // Build response
    let scopes: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM memory_scopes WHERE memory_id = ? ORDER BY scope"
    )
    .bind(&pre_tx.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let tags: Vec<String> = serde_json::from_str(&pre_tx.tags_json).unwrap_or_default();
    let metadata: serde_json::Value = serde_json::from_str(&pre_tx.metadata_json).unwrap_or(json!({}));

    Ok(AddMemoryResponse {
        id: pre_tx.id.clone(),
        memory_type: pre_tx.memory_type_str.clone(),
        content: pre_tx.content.clone(),
        scopes,
        tags,
        importance: pre_tx.importance,
        created_at: pre_tx.now.clone(),
        quality_score: Some(pre_tx.quality.score),
        metadata,
        suggested_links: Vec::new(),
        duplicate_warning: None,
        context: pre_tx.context.clone(),
        title: pre_tx.title.clone(),
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

/// Prepare pre-transaction data from request
/// This extracts all the preparation logic that was in voidm-core::add_memory
pub async fn prepare_add_memory_data(
    pool: &SqlitePool,
    mut req: voidm_db::models::AddMemoryRequest,
    config: &voidm_core::Config,
) -> Result<PreTxData> {
    use uuid::Uuid;
    use chrono::Utc;
    use voidm_core::embeddings;
    
    // Auto-enrich tags BEFORE creating tags_json (moved to beginning)
    // TODO: Re-enable when auto_tagger_tinyllama is fixed
    
    // Redact secrets from memory content and metadata BEFORE insertion
    let mut redaction_warnings = Vec::new();
    if let Err(e) = voidm_core::crud::redact_memory(&mut req, config, &mut redaction_warnings) {
        tracing::warn!("Failed to redact secrets: {}. Continuing without redaction.", e);
    }
    
    // Log any redacted secrets to inform user
    for warning in &redaction_warnings {
        tracing::warn!(
            "Redacted {} {}(s) in memory.{}: {}",
            warning.count, warning.pattern_type, warning.field, warning.count
        );
    }
    
    let id = req.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = Utc::now().to_rfc3339();
    
    // Set author="user" unless already set
    if !req.metadata.get("author").is_some() {
        req.metadata["author"] = serde_json::json!("user");
    }
    
    let tags_json = serde_json::to_string(&req.tags)?;
    let metadata_json = serde_json::to_string(&req.metadata)?;
    let memory_type_str = req.memory_type.to_string();

    // 1. Compute embedding OUTSIDE transaction with consistent chunking
    let embedding_result = if config.embeddings.enabled {
        match embeddings::embed_text_chunked(&config.embeddings.model, &req.content, embeddings::DEFAULT_CHUNK_SIZE) {
            Ok(emb) => Some(emb),
            Err(e) => {
                tracing::warn!("Failed to compute embedding: {}. Skipping vector storage.", e);
                None
            }
        }
    } else {
        None
    };

    // Compute quality score OUTSIDE transaction (will persist to DB)
    let memory_type_enum = req.memory_type.clone();
    let quality_mt = voidm_core::crud::convert_memory_type(&memory_type_enum);
    let quality = voidm_scoring::compute_quality_score(&req.content, &quality_mt);

    // Ensure vec_memories table exists with correct dimension
    if let Some(ref emb) = embedding_result {
        let dim = emb.len();
        voidm_core::vector::ensure_vector_table(pool, dim).await?;
        // Record in db_meta
        sqlx::query("INSERT OR REPLACE INTO db_meta (key, value) VALUES ('embedding_model', ?)")
            .bind(&config.embeddings.model)
            .execute(pool)
            .await?;
        sqlx::query("INSERT OR REPLACE INTO db_meta (key, value) VALUES ('embedding_dim', ?)")
            .bind(dim.to_string())
            .execute(pool)
            .await?;
    }

    // Validate --link targets exist before opening transaction
    // Also resolve short IDs to full IDs for later use
    let mut resolved_link_targets = Vec::new();
    for link in &req.links {
        // Use resolve_id_sqlite to support short ID prefixes (minimum 4 chars)
        let target_id = voidm_core::crud::resolve_id_sqlite(pool, &link.target_id).await?;
        resolved_link_targets.push((link.edge_type.clone(), link.note.clone(), target_id));
        
        if link.edge_type.requires_note() && link.note.is_none() {
            anyhow::bail!(
                "RELATES_TO requires --note explaining why no stronger relationship applies."
            );
        }
    }

    Ok(PreTxData {
        id,
        memory_type_str,
        content: req.content,
        importance: req.importance,
        tags_json,
        metadata_json,
        context: req.context,
        scopes: req.scopes,
        quality,
        embedding_result,
        resolved_link_targets,
        now,
        title: req.title,
    })
}
