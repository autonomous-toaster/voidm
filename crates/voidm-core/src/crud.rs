use anyhow::{Context, Result, bail};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{
    AddMemoryRequest, AddMemoryResponse, DuplicateWarning, EdgeType, LinkResponse,
    Memory,
};
use voidm_scoring;
use crate::{embeddings, search, vector, redactor};
use crate::config::Config;

/// Convert voidm_core MemoryType to voidm_scoring MemoryType
pub fn convert_memory_type(mt: &crate::models::MemoryType) -> voidm_scoring::MemoryType {
    match mt {
        crate::models::MemoryType::Episodic => voidm_scoring::MemoryType::Episodic,
        crate::models::MemoryType::Semantic => voidm_scoring::MemoryType::Semantic,
        crate::models::MemoryType::Procedural => voidm_scoring::MemoryType::Procedural,
        crate::models::MemoryType::Conceptual => voidm_scoring::MemoryType::Conceptual,
        crate::models::MemoryType::Contextual => voidm_scoring::MemoryType::Contextual,
    }
}

/// Resolve a full or short (prefix) ID to a full memory ID (backend-agnostic).
/// 
/// # Backend Abstraction
/// This function accepts any type implementing the Database trait, allowing
/// it to work with SQLite, PostgreSQL, Neo4j, or other backends.
/// 
/// - If `id` is already a full UUID that exists → return it as-is.
/// - If `id` is a prefix → find all matches; error if 0 or >1.
/// - Minimum prefix length: 4 characters.
pub async fn resolve_id<D: voidm_db::Database + ?Sized>(db: &D, id: &str) -> Result<String> {
    db.resolve_memory_id(id).await
}

/// Resolve a full or short (prefix) ID to a full memory ID (SQLite-specific).
/// 
/// # Deprecated
/// Use `resolve_id()` with Database trait instead for backend-agnostic code.
/// This function is kept for compatibility with existing CLI code.
pub async fn resolve_id_sqlite(pool: &SqlitePool, id: &str) -> Result<String> {
    if id.len() < 4 {
        anyhow::bail!("ID prefix too short (minimum 4 characters)");
    }
    
    let row = sqlx::query_scalar::<_, String>("SELECT id FROM memories WHERE id LIKE ? LIMIT 1")
        .bind(format!("{}%", id))
        .fetch_optional(pool)
        .await
        .context("Failed to resolve ID")?;

    row.context("ID not found")
}

/// Add a memory — full workflow:
/// 1. Compute embedding + quality_score (outside tx)
/// 2. BEGIN tx
/// 3. Insert memory + scopes + FTS + vec + graph node + links
/// 4. COMMIT
/// Returns AddMemoryResponse with suggested_links and duplicate_warning.
// Get a single memory by ID.
/// Get a memory by ID (backend-agnostic via Database trait)
///
/// # Arguments
/// * `db` - Any type implementing Database trait
/// * `id` - Memory ID (full UUID or 4+ char prefix)
///
/// Returns the Memory if found, None if not found
pub async fn get_memory<D: voidm_db::Database + ?Sized>(db: &D, id: &str) -> Result<Option<Memory>> {
    match db.get_memory(id).await? {
        None => Ok(None),
        Some(value) => {
            let memory: Memory = serde_json::from_value(value)?;
            Ok(Some(memory))
        }
    }
}

/// Get a memory by ID from SqlitePool (for backward compatibility during Phase 1)
/// TODO: Remove in Phase 1.5 when add_memory is refactored
pub async fn get_memory_sqlite(pool: &SqlitePool, id: &str) -> Result<Option<Memory>> {
    // Resolve short IDs to full IDs (supports 4+ char prefixes)
    let full_id = if id.len() < 4 {
        id.to_string()
    } else {
        match resolve_id_sqlite(pool, id).await {
            Ok(resolved) => resolved,
            Err(_) => {
                return Ok(None);
            }
        }
    };

    let row: Option<(String, String, String, i64, String, String, Option<f32>, Option<String>, String, String)> = sqlx::query_as(
        "SELECT id, type, content, importance, tags, metadata, quality_score, context, created_at, updated_at
         FROM memories WHERE id = ?"
    )
    .bind(&full_id)
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some((id, memory_type, content, importance, tags_json, metadata_json, quality_score_db, context, created_at, updated_at)) => {
            let scopes = get_scopes(pool, &id).await?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap_or_default();
            
            // Use persisted quality_score if available, otherwise compute
            let quality_score = if let Some(score) = quality_score_db {
                Some(score)
            } else {
                let memory_type_enum: crate::models::MemoryType = memory_type.parse().unwrap_or(crate::models::MemoryType::Semantic);
                let quality_mt = convert_memory_type(&memory_type_enum);
                let quality_score_val = voidm_scoring::compute_quality_score(&content, &quality_mt);
                Some(quality_score_val.score)
            };
            
            Ok(Some(Memory {
                id,
                memory_type,
                content,
                importance,
                tags,
                metadata,
                scopes,
                created_at,
                updated_at,
                quality_score,
                context,
                title: None,
            }))
        }
    }
}

/// List memories newest-first (backend-agnostic via Database trait)
///
/// # Arguments
/// * `db` - Any type implementing Database trait
/// * `limit` - Maximum number of memories to return
///
/// Returns a list of memories ordered by creation date
pub async fn list_memories<D: voidm_db::Database + ?Sized>(db: &D, limit: Option<usize>) -> Result<Vec<Memory>> {
    let values = db.list_memories(limit).await?;
    let memories: Vec<Memory> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    Ok(memories)
}

/// Delete a memory and all its graph edges (cascade via FK).
/// Delete a memory by ID (backend-agnostic via Database trait)
///
/// # Arguments
/// * `db` - Any type implementing Database trait
/// * `id` - Memory ID (full UUID or 4+ char prefix)
///
/// Returns true if memory was deleted, false if not found
pub async fn delete_memory<D: voidm_db::Database + ?Sized>(db: &D, id: &str) -> Result<bool> {
    db.delete_memory(id).await
}


/// Get all scopes for a memory.
pub async fn get_scopes(pool: &SqlitePool, memory_id: &str) -> Result<Vec<String>> {
    let scopes: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM memory_scopes WHERE memory_id = ? ORDER BY scope"
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await?;
    Ok(scopes)
}

/// List all known scope strings.
pub async fn list_scopes(pool: &SqlitePool) -> Result<Vec<String>> {
    let scopes: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT scope FROM memory_scopes ORDER BY scope"
    )
    .fetch_all(pool)
    .await?;
    Ok(scopes)
}

/// Create a graph edge between two memories.
/// Create a graph edge between two memories (backend-agnostic)
pub async fn link_memories<D: voidm_db::Database + ?Sized>(
    db: &D,
    from_id: &str,
    edge_type: &EdgeType,
    to_id: &str,
    note: Option<&str>,
) -> Result<LinkResponse> {
    // Validate RELATES_TO requires note
    if edge_type.requires_note() && note.is_none() {
        anyhow::bail!("RELATES_TO requires --note explaining why no stronger relationship applies.");
    }

    // Resolve IDs (support short prefixes)
    let from_id = db.resolve_memory_id(from_id).await?;
    let to_id = db.resolve_memory_id(to_id).await?;

    // Call trait method - returns JSON response with resolved IDs
    let response_json = db.link_memories(&from_id, edge_type.as_str(), &to_id, note).await?;
    
    // Convert JSON back to LinkResponse
    let created = response_json.get("created").and_then(|v| v.as_bool()).unwrap_or(false);
    let from = response_json.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let rel = response_json.get("rel").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let to = response_json.get("to").and_then(|v| v.as_str()).unwrap_or("").to_string();

    Ok(LinkResponse {
        created,
        from,
        rel,
        to,
    })
}

/// Create a graph edge between two memories using SqlitePool (for backward compatibility)
/// TODO: Remove in Phase 1.5 when add_memory is refactored
pub async fn link_memories_sqlite(
    pool: &SqlitePool,
    from_id: &str,
    edge_type: &EdgeType,
    to_id: &str,
    note: Option<&str>,
) -> Result<LinkResponse> {
    // Validate RELATES_TO requires note
    if edge_type.requires_note() && note.is_none() {
        anyhow::bail!("RELATES_TO requires --note explaining why no stronger relationship applies.");
    }

    // Check both memories exist
    // Support short ID prefixes (minimum 4 chars) for flexibility
    let from_id = resolve_id_sqlite(pool, from_id).await?;
    let to_id = resolve_id_sqlite(pool, to_id).await?;

    // Get or create graph nodes
    let from_node = get_or_create_node(pool, &from_id).await?;
    let to_node = get_or_create_node(pool, &to_id).await?;

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
         VALUES (?, ?, ?, ?, ?)"
    )
    .bind(from_node)
    .bind(to_node)
    .bind(edge_type.as_str())
    .bind(note)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(LinkResponse {
        created: true,
        from: from_id.to_string(),
        rel: edge_type.as_str().to_string(),
        to: to_id.to_string(),
    })
}

/// Remove a graph edge.
pub async fn unlink_memories(
    pool: &SqlitePool,
    from_id: &str,
    edge_type: &EdgeType,
    to_id: &str,
) -> Result<bool> {
    // Resolve short IDs to full IDs (supports 4+ char prefixes)
    let from_id = resolve_id_sqlite(pool, from_id).await?;
    let to_id = resolve_id_sqlite(pool, to_id).await?;
    
    // Get node IDs
    let from_node_opt: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM graph_nodes WHERE memory_id = ?"
    )
    .bind(&from_id)
    .fetch_optional(pool)
    .await?;

    let from_node = match from_node_opt {
        Some(id) => id,
        None => return Ok(false),
    };

    let to_node_opt: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM graph_nodes WHERE memory_id = ?"
    )
    .bind(&to_id)
    .fetch_optional(pool)
    .await?;

    let to_node = match to_node_opt {
        Some(id) => id,
        None => return Ok(false),
    };

    // Delete the edge
    let result = sqlx::query(
        "DELETE FROM graph_edges WHERE source_id = ? AND target_id = ? AND rel_type = ?"
    )
    .bind(from_node)
    .bind(to_node)
    .bind(edge_type.as_str())
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

async fn get_or_create_node(pool: &SqlitePool, memory_id: &str) -> Result<i64> {
    sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
        .bind(memory_id)
        .execute(pool)
        .await?;
    let node_id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(memory_id)
        .fetch_one(pool)
        .await?;
    Ok(node_id)
}

async fn intern_property_key(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, key: &str) -> Result<i64> {
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

/// Check model mismatch against db_meta.
pub async fn check_model_mismatch(pool: &SqlitePool, _configured_model: &str) -> Result<Option<(String, String)>> {
    // SQLite implementation: No persistent model metadata stored
    // Return None (no mismatch) - model tracking would need schema addition
    Ok(None)
}

/// List all memory-to-memory edges for migration purposes
pub async fn list_edges(pool: &SqlitePool) -> Result<Vec<crate::models::MemoryEdge>> {
    // Get all edges with their source and target memory IDs
    let edges_data: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT gn1.memory_id, gn2.memory_id, ge.rel_type, ge.note
        FROM graph_edges ge
        JOIN graph_nodes gn1 ON ge.source_id = gn1.id
        JOIN graph_nodes gn2 ON ge.target_id = gn2.id
        ORDER BY ge.created_at
        "#
    )
    .fetch_all(pool)
    .await?;

    let edges = edges_data.into_iter().map(|(from_id, to_id, rel_type, note)| {
        crate::models::MemoryEdge {
            from_id,
            to_id,
            rel_type,
            note,
        }
    }).collect();

    Ok(edges)
}

/// Redact secrets from memory content, tags, and metadata in-place.
/// Returns list of redaction warnings.
pub fn redact_memory(
    req: &mut AddMemoryRequest,
    config: &Config,
    warnings: &mut Vec<redactor::RedactionWarning>,
) -> Result<()> {
    // Redact content
    let (redacted_content, content_warnings) = redactor::redact_text(&req.content, &config.redaction);
    for mut w in content_warnings {
        w.field = "content".to_string();
        warnings.push(w);
    }
    req.content = redacted_content;

    // Redact tags
    let mut redacted_tags = Vec::new();
    for tag in &req.tags {
        let (redacted_tag, tag_warnings) = redactor::redact_text(tag, &config.redaction);
        for mut w in tag_warnings {
            w.field = "tags".to_string();
            warnings.push(w);
        }
        redacted_tags.push(redacted_tag);
    }
    req.tags = redacted_tags;

    // Redact metadata
    if let Ok(metadata_str) = serde_json::to_string(&req.metadata) {
        let (redacted_metadata_str, metadata_warnings) = redactor::redact_text(&metadata_str, &config.redaction);
        for mut w in metadata_warnings {
            w.field = "metadata".to_string();
            warnings.push(w);
        }
        if let Ok(redacted_metadata) = serde_json::from_str(&redacted_metadata_str) {
            req.metadata = redacted_metadata;
        }
    }

    Ok(())
}

/// Extract named entities from memory content and link to concepts.
/// Creates INSTANCE_OF edges between the memory and extracted concepts.
/// Optionally creates missing concepts if `auto_create` is true.
#[cfg(feature = "ner")]
async fn extract_and_link_concepts(
    pool: &SqlitePool,
    memory_id: &str,
    content: &str,
    min_score: f32,
    auto_create: bool,
) -> Result<()> {
    use crate::ner;

    // Ensure NER model is loaded (downloads on first use)
    ner::ensure_ner_model().await?;

    // Extract entities from content
    let entities = ner::extract_entities(content)?;

    // Filter by minimum score
    let filtered_entities: Vec<_> = entities.iter()
        .filter(|e| e.score >= min_score)
        .collect();

    if filtered_entities.is_empty() {
        tracing::debug!("No entities above min_score {:.2} extracted from memory {}", min_score, memory_id);
        return Ok(());
    }

    tracing::info!("Extracted {} entities from memory {} (min_score: {:.2})", filtered_entities.len(), memory_id, min_score);

    // For each entity, find or create concept and link
    for entity in filtered_entities {
        // Try to find existing concept by name (case-insensitive)
        let existing_concept: Option<String> = sqlx::query_scalar(
            "SELECT id FROM ontology_concepts WHERE lower(name) = lower(?)"
        )
        .bind(&entity.text)
        .fetch_optional(pool)
        .await?;

        let concept_id = if let Some(id) = existing_concept {
            id
        } else if auto_create {
            // Create new concept
            let new_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            
            sqlx::query(
                "INSERT INTO ontology_concepts (id, name, description, scope, created_at)
                 VALUES (?, ?, ?, ?, ?)"
            )
            .bind(&new_id)
            .bind(&entity.text)
            .bind(format!("Auto-extracted from memory: {} ({:.2} confidence)", entity.entity_type, entity.score))
            .bind::<Option<String>>(None)
            .bind(&now)
            .execute(pool)
            .await?;

            // Also insert into FTS for searchability
            sqlx::query("INSERT INTO ontology_concept_fts (id, name, description) VALUES (?, ?, ?)")
                .bind(&new_id)
                .bind(&entity.text)
                .bind(format!("Auto-extracted: {}", entity.entity_type))
                .execute(pool)
                .await?;

            tracing::debug!("Created concept '{}' for memory {}", entity.text, memory_id);
            new_id
        } else {
            tracing::debug!("Concept '{}' not found and auto_create disabled, skipping", entity.text);
            continue;
        };

        // Create INSTANCE_OF edge from memory to concept
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO ontology_edges (from_id, from_type, rel_type, to_id, to_type, note, created_at)
             VALUES (?, 'memory', 'INSTANCE_OF', ?, 'concept', ?, ?)"
        )
        .bind(memory_id)
        .bind(&concept_id)
        .bind(format!("Extracted with {:.2} confidence", entity.score))
        .bind(&now)
        .execute(pool)
        .await?;

        tracing::debug!("Linked memory {} to concept '{}'", memory_id, entity.text);
    }

    Ok(())
}

/// Stub for when NER feature is disabled
#[cfg(not(feature = "ner"))]
async fn extract_and_link_concepts(
    _pool: &SqlitePool,
    _memory_id: &str,
    _content: &str,
    _min_score: f32,
    _auto_create: bool,
) -> Result<()> {
    tracing::debug!("Concept extraction skipped (NER feature not enabled)");
    Ok(())
}

// Trait-based wrappers for CLI compatibility
// These allow commands to use &Arc<dyn Database> while maintaining backward compat

