use anyhow::{Context, Result, bail};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{
    AddMemoryRequest, AddMemoryResponse, DuplicateWarning, EdgeType, LinkResponse,
    ConflictWarning, Memory,
};
use voidm_scoring;
use crate::{embeddings, search, vector, redactor};
use crate::config::Config;

/// Convert voidm_core MemoryType to voidm_scoring MemoryType
fn convert_memory_type(mt: &crate::models::MemoryType) -> voidm_scoring::MemoryType {
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
pub async fn resolve_id<D: voidm_db_trait::Database + ?Sized>(db: &D, id: &str) -> Result<String> {
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
pub async fn add_memory(pool: &SqlitePool, mut req: AddMemoryRequest, config: &Config) -> Result<AddMemoryResponse> {
    // Auto-enrich tags BEFORE creating tags_json (moved to beginning)
    // TODO: Re-enable when auto_tagger_tinyllama is fixed
    // #[cfg(feature = "tinyllama")]
    // {
    //     if let Err(e) = auto_tagger_tinyllama::enrich_memory_tags_tinyllama(&mut req, config).await {
    //         tracing::warn!("Failed to auto-enrich tags: {}. Using user-provided tags only.", e);
    //     }
    // }
    
    // Redact secrets from memory content and metadata BEFORE insertion
    let mut redaction_warnings = Vec::new();
    if let Err(e) = redact_memory(&mut req, config, &mut redaction_warnings) {
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
    let quality_mt = convert_memory_type(&memory_type_enum);
    let quality = voidm_scoring::compute_quality_score(&req.content, &quality_mt);

    // Ensure vec_memories table exists with correct dimension
    if let Some(ref emb) = embedding_result {
        let dim = emb.len();
        vector::ensure_vector_table(pool, dim).await?;
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
    for link in &req.links {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
            .bind(&link.target_id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            anyhow::bail!("Link target '{}' not found", link.target_id);
        }
        if link.edge_type.requires_note() && link.note.is_none() {
            anyhow::bail!(
                "RELATES_TO requires --note explaining why no stronger relationship applies."
            );
        }
    }

    // 2–8: Atomic transaction
    let mut tx = pool.begin().await?;

    // Insert memory with persistent quality_score
    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, quality_score, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&memory_type_str)
    .bind(&req.content)
    .bind(req.importance)
    .bind(&tags_json)
    .bind(&metadata_json)
    .bind(quality.score)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .context("Failed to insert memory")?;

    // Insert scopes
    for scope in &req.scopes {
        sqlx::query("INSERT OR IGNORE INTO memory_scopes (memory_id, scope) VALUES (?, ?)")
            .bind(&id)
            .bind(scope)
            .execute(&mut *tx)
            .await?;
    }

    // Insert FTS
    sqlx::query("INSERT INTO memories_fts (id, content) VALUES (?, ?)")
        .bind(&id)
        .bind(&req.content)
        .execute(&mut *tx)
        .await?;

    // Insert embedding
    if let Some(ref emb) = embedding_result {
        let bytes: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
        sqlx::query(
            "INSERT INTO vec_memories (memory_id, embedding) VALUES (?, ?)"
        )
        .bind(&id)
        .bind(&bytes)
        .execute(&mut *tx)
        .await
        .context("Failed to insert into vec_memories")?;
    }

    // Graph node upsert
    sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let node_id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO graph_node_labels (node_id, label) VALUES (?, 'Memory')")
        .bind(node_id)
        .execute(&mut *tx)
        .await?;

    // Store memory_type as a text property on the graph node
    let key_id = intern_property_key(&mut tx, "memory_type").await?;
    sqlx::query(
        "INSERT OR REPLACE INTO graph_node_props_text (node_id, key_id, value) VALUES (?, ?, ?)"
    )
    .bind(node_id)
    .bind(key_id)
    .bind(&memory_type_str)
    .execute(&mut *tx)
    .await?;

    // Create --link edges
    for link in &req.links {
        let target_node: i64 = sqlx::query_scalar(
            "SELECT n.id FROM graph_nodes n WHERE n.memory_id = ?"
        )
        .bind(&link.target_id)
        .fetch_one(&mut *tx)
        .await
        .with_context(|| format!("Graph node not found for target '{}'", link.target_id))?;

        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(node_id)
        .bind(target_node)
        .bind(link.edge_type.as_str())
        .bind(&link.note)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await.context("Transaction commit failed")?;

    // Post-insert: Auto-extract and link concepts (if enabled)
    if config.insert.auto_extract_concepts {
        if let Err(e) = extract_and_link_concepts(
            pool,
            &id,
            &req.content,
            config.insert.concept_min_score,
            config.insert.concept_auto_create,
        ).await {
            tracing::warn!("Failed to auto-extract concepts: {}. Continuing with memory creation.", e);
        }
    }

    // Post-insert: Auto-link memories with shared tags
    if !req.tags.is_empty() {
        let tag_limit = config.insert.auto_link_limit;
        if let Err(e) = voidm_tagging::auto_link_by_tags(pool, &id, &req.tags, tag_limit).await {
            tracing::warn!("Failed to auto-link by tags: {}. Continuing with memory creation.", e);
        }
    }

    // Post-insert: Check for auto-merge (very high similarity > 0.98)
    // For episodic memories, also check temporal proximity
    if let Some(ref emb) = embedding_result {
        let merge_candidates = search::find_similar(
            pool, emb, &id, 1, config.insert.automerge_threshold,
        ).await.unwrap_or_default();

        if let Some((merge_id, merge_score)) = merge_candidates.first() {
            if let Ok(Some(dup_mem)) = get_memory(pool, merge_id).await {
                // For episodic memories, check temporal compatibility
                let should_merge = if memory_type_str == "episodic" {
                    let (temporal_ok, reason) = check_episodic_temporal_compatibility(
                        &now,
                        &dup_mem.created_at,
                        config,
                    );
                    if !temporal_ok && config.insert.episodic.preserve_temporal_separation {
                        tracing::debug!("Episodic memory outside temporal window ({}): preserving separation", reason);
                        false
                    } else {
                        true
                    }
                } else {
                    true
                };

                if should_merge {
                    // Auto-merge: consolidate tags and return merged ID
                    if let Err(e) = merge_memories(pool, &id, merge_id, &req.tags, &dup_mem.tags).await {
                        tracing::warn!("Failed to merge memories: {}. Keeping both.", e);
                    } else {
                        println!("Duplicate, merged with {}", merge_id);
                        return Ok(AddMemoryResponse {
                            id: merge_id.clone(),
                            memory_type: memory_type_str,
                            content: dup_mem.content,
                            scopes: dup_mem.scopes,
                            tags: dup_mem.tags,
                            importance: dup_mem.importance,
                            created_at: dup_mem.created_at,
                            quality_score: dup_mem.quality_score,
                            metadata: dup_mem.metadata,
                            suggested_links: vec![],
                            duplicate_warning: None,
                        });
                    }
                }
            }
        }
    }

    // Post-insert: compute suggested_links and duplicate_warning (outside tx)
    let (suggested_links, duplicate_warning) = if let Some(ref emb) = embedding_result {
        let dup_candidates = search::find_similar(
            pool, emb, &id, 1, config.insert.duplicate_threshold,
        ).await.unwrap_or_default();

        let dup_warning = if let Some((dup_id, dup_score)) = dup_candidates.first() {
            if let Ok(Some(dup_mem)) = get_memory(pool, dup_id).await {
                let content_trunc = if dup_mem.content.len() > 120 {
                    format!("{}...", crate::search::safe_truncate(&dup_mem.content, 120))
                } else {
                    dup_mem.content.clone()
                };
                Some(DuplicateWarning {
                    id: dup_id.clone(),
                    score: *dup_score,
                    content: content_trunc,
                    message: "Near-duplicate detected. Consider linking instead of inserting.".into(),
                })
            } else {
                None
            }
        } else {
            None
        };

        let link_candidates = search::find_similar(
            pool, emb, &id,
            config.insert.auto_link_limit,
            config.insert.auto_link_threshold,
        ).await.unwrap_or_default();

        let suggested = search::build_suggested_links(pool, &memory_type_str, link_candidates)
            .await
            .unwrap_or_default();

        (suggested, dup_warning)
    } else {
        (vec![], None)
    };

    Ok(AddMemoryResponse {
        id,
        memory_type: memory_type_str,
        content: req.content,
        scopes: req.scopes,
        tags: req.tags,
        importance: req.importance,
        created_at: now,
        quality_score: Some(quality.score),
        metadata: req.metadata,
        suggested_links,
        duplicate_warning,
    })
}

/// Check if two episodic memories should be merged based on temporal proximity.
/// Returns (should_merge, reason)
fn check_episodic_temporal_compatibility(
    new_created_at: &str,
    existing_created_at: &str,
    config: &Config,
) -> (bool, &'static str) {
    // Parse RFC3339 timestamps
    if let (Ok(new_time), Ok(existing_time)) = (
        chrono::DateTime::parse_from_rfc3339(new_created_at),
        chrono::DateTime::parse_from_rfc3339(existing_created_at),
    ) {
        let duration = (new_time.timestamp() - existing_time.timestamp()).abs() as u64;
        let temporal_window = config.insert.episodic.temporal_window_secs;

        if duration <= temporal_window {
            (true, "within temporal window")
        } else {
            (false, "outside temporal window")
        }
    } else {
        // If we can't parse timestamps, use default merge behavior
        (true, "timestamps unparseable, merging by default")
    }
}

/// Merge a new memory into an existing memory with tag consolidation.
/// Returns the target memory ID.
async fn merge_memories(
    pool: &SqlitePool,
    new_id: &str,
    target_id: &str,
    new_tags: &[String],
    target_tags: &[String],
) -> Result<String> {
    // Consolidate tags: deduplicate across both
    let mut merged_tags = target_tags.to_vec();
    for tag in new_tags {
        let normalized = tag.trim().to_lowercase();
        if !merged_tags.iter().any(|t| t.trim().to_lowercase() == normalized) {
            merged_tags.push(tag.clone());
        }
    }

    // Update target tags
    let tags_json = serde_json::to_string(&merged_tags)?;
    sqlx::query("UPDATE memories SET tags = ? WHERE id = ?")
        .bind(&tags_json)
        .bind(target_id)
        .execute(pool)
        .await?;

    // Delete all graph edges from new_id
    let new_node: Option<i64> = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
        .bind(new_id)
        .fetch_optional(pool)
        .await?;

    if let Some(node_id) = new_node {
        sqlx::query("DELETE FROM graph_edges WHERE source_id = ? OR target_id = ?")
            .bind(node_id)
            .bind(node_id)
            .execute(pool)
            .await?;

        // Delete graph node
        sqlx::query("DELETE FROM graph_nodes WHERE id = ?")
            .bind(node_id)
            .execute(pool)
            .await?;
    }

    // Delete memory and related tables
    sqlx::query("DELETE FROM memories_fts WHERE id = ?")
        .bind(new_id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM vec_memories WHERE memory_id = ?")
        .bind(new_id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM memory_scopes WHERE memory_id = ?")
        .bind(new_id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM memories WHERE id = ?")
        .bind(new_id)
        .execute(pool)
        .await?;

    Ok(target_id.to_string())
}

/// Get a single memory by ID.
pub async fn get_memory(pool: &SqlitePool, id: &str) -> Result<Option<Memory>> {
    let row: Option<(String, String, String, i64, String, String, Option<f32>, String, String)> = sqlx::query_as(
        "SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
         FROM memories WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some((id, memory_type, content, importance, tags_json, metadata_json, quality_score_db, created_at, updated_at)) => {
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
            }))
        }
    }
}

/// List memories newest-first, with optional scope prefix and type filter.
pub async fn list_memories(
    pool: &SqlitePool,
    scope_filter: Option<&str>,
    type_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<Memory>> {
    // Build query dynamically
    let rows: Vec<(String, String, String, i64, String, String, Option<f32>, String, String)> = if let Some(scope) = scope_filter {
        let scope_prefix = format!("{}%", scope);
        sqlx::query_as(
            "SELECT DISTINCT m.id, m.type, m.content, m.importance, m.tags, m.metadata, m.quality_score, m.created_at, m.updated_at
             FROM memories m
             JOIN memory_scopes ms ON ms.memory_id = m.id
             WHERE ms.scope LIKE ?
             ORDER BY m.created_at DESC LIMIT ?"
        )
        .bind(&scope_prefix)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, type, content, importance, tags, metadata, quality_score, created_at, updated_at
             FROM memories ORDER BY created_at DESC LIMIT ?"
        )
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    };

    let mut memories = Vec::new();
    for (id, memory_type, content, importance, tags_json, metadata_json, quality_score_db, created_at, updated_at) in rows {
        if let Some(t) = type_filter {
            if memory_type != t { continue; }
        }
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
        
        memories.push(Memory { 
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
        });
    }
    Ok(memories)
}

/// Delete a memory and all its graph edges (cascade via FK).
pub async fn delete_memory(pool: &SqlitePool, id: &str) -> Result<bool> {
    // Check if memory exists first
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?)")
        .bind(id)
        .fetch_one(pool)
        .await?;
    
    if !exists {
        return Ok(false);
    }

    // Delete in order of dependencies (FKs cascade)
    sqlx::query("DELETE FROM graph_edges WHERE from_id = ? OR to_id = ?")
        .bind(id)
        .bind(id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM memory_scopes WHERE memory_id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM vec_memories WHERE memory_id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM memories_fts WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM memories WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(true)
}


/// Get all scopes for a memory.
async fn get_scopes(pool: &SqlitePool, memory_id: &str) -> Result<Vec<String>> {
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
pub async fn link_memories(
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
    for id in &[from_id, to_id] {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            anyhow::bail!("Memory '{}' not found", id);
        }
    }

    // Get or create graph nodes
    let from_node = get_or_create_node(pool, from_id).await?;
    let to_node = get_or_create_node(pool, to_id).await?;

    // Check for conflicting edge
    let conflict_rel = edge_type.conflict();
    let conflict_warning = if let Some(opposing) = conflict_rel {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM graph_edges WHERE source_id = ? AND target_id = ? AND rel_type = ?"
        )
        .bind(from_node)
        .bind(to_node)
        .bind(opposing)
        .fetch_optional(pool)
        .await?;

        if exists.is_some() {
            Some(ConflictWarning {
                existing_rel: opposing.to_string(),
                message: format!(
                    "Conflict: {} {} {} already exists. Both edges are now present. Use 'voidm unlink {} {} {}' to resolve.",
                    from_id, opposing, to_id, from_id, opposing, to_id
                ),
            })
        } else {
            None
        }
    } else {
        None
    };

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
        conflict_warning,
    })
}

/// Remove a graph edge.
pub async fn unlink_memories(
    pool: &SqlitePool,
    from_id: &str,
    edge_type: &EdgeType,
    to_id: &str,
) -> Result<bool> {
    // Get node IDs
    let from_node_opt: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM graph_nodes WHERE memory_id = ?"
    )
    .bind(from_id)
    .fetch_optional(pool)
    .await?;

    let from_node = match from_node_opt {
        Some(id) => id,
        None => return Ok(false),
    };

    let to_node_opt: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM graph_nodes WHERE memory_id = ?"
    )
    .bind(to_id)
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

/// List all ontology edges for migration purposes
pub async fn list_ontology_edges(pool: &SqlitePool) -> Result<Vec<crate::models::OntologyEdgeForMigration>> {
    let edges_data: Vec<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT from_id, from_type, to_id, to_type, rel_type, note
        FROM ontology_edges
        ORDER BY from_id
        "#
    )
    .fetch_all(pool)
    .await?;

    let edges = edges_data.into_iter().map(|(from_id, from_type, to_id, to_type, rel_type, note)| {
        crate::models::OntologyEdgeForMigration {
            from_id,
            from_type,
            to_id,
            to_type,
            rel_type,
            note,
        }
    }).collect();

    Ok(edges)
}

/// Redact secrets from memory content, tags, and metadata in-place.
/// Returns list of redaction warnings.
fn redact_memory(
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

