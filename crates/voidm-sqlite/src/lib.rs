//! SQLite backend for voidm using the generic Database trait
//!
//! This crate provides a lightweight SQLite backend implementation
//! that uses the QueryTranslator abstraction from voidm-core for all SQL generation.
//!
//! Key pattern: JSON ↔ CypherOperation ↔ SQL Translation
//! - Incoming JSON trait calls converted to CypherOperation
//! - SqliteTranslator generates backend-specific SQL
//! - Results converted back to JSON for trait consumers

use anyhow::{Context, Result};
use chrono::Utc;
use once_cell::sync::OnceCell;
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::path::Path;
use std::pin::Pin;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;
use tracing;
use voidm_db::Database;

pub mod add_memory_backend;
pub mod chunk_nodes;
pub mod migrate;
pub mod utils;
pub mod deprecated;

/// Load sqlite-vec at process level via sqlite3_auto_extension
fn ensure_sqlite_vec_loaded() {
    static LOADED: OnceCell<()> = OnceCell::new();
    LOADED.get_or_init(|| unsafe {
        libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

/// Open or create SQLite connection pool
pub async fn open_pool(db_path: &Path) -> Result<SqlitePool> {
    ensure_sqlite_vec_loaded();

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create directory {}", parent.display()))?;
    }

    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .with_context(|| format!("Cannot open database at {}", db_path.display()))?;

    // Initialize schema
    let db = SqliteDatabase::new(pool.clone());
    db.ensure_schema().await?;

    Ok(pool)
}

/// SQLite database backend using generic Database trait
#[derive(Clone)]
pub struct SqliteDatabase {
    pub pool: SqlitePool,
}

impl SqliteDatabase {
    /// Create new instance with existing pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Delete a memory and all associated data (internal implementation)

    /// Get a memory by ID (internal implementation, extracted from voidm-core)
    async fn get_memory_impl(&self, id: &str) -> Result<Option<voidm_db::models::Memory>> {
        // Resolve short IDs to full IDs (supports 4+ char prefixes)
        let full_id = if id.len() < 4 {
            // If less than 4 chars, can't be a valid short ID, treat as exact match attempt
            id.to_string()
        } else {
            match utils::resolve_id_sqlite(&self.pool, id).await {
                Ok(resolved) => resolved,
                Err(_) => {
                    // If resolution fails, it means ID not found
                    return Ok(None);
                }
            }
        };

        let row: Option<(String, String, String, i64, String, String, Option<f32>, Option<String>, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, type, content, importance, tags, metadata, quality_score, context, created_at, updated_at, title
             FROM memories WHERE id = ?"
        )
        .bind(&full_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(None),
            Some((id, memory_type, content, importance, tags_json, metadata_json, quality_score_db, context, created_at, updated_at, title)) => {
                let scopes = utils::get_scopes(&self.pool, &id).await?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap_or_default();
                
                // Use persisted quality_score if available, otherwise compute
                let quality_score = if let Some(score) = quality_score_db {
                    Some(score)
                } else {
                    let memory_type_enum: voidm_db::models::MemoryType = memory_type.parse().unwrap_or(voidm_db::models::MemoryType::Semantic);
                    let quality_mt = voidm_core::crud::convert_memory_type(&memory_type_enum);
                    let quality_score_val = voidm_scoring::compute_quality_score(&content, &quality_mt);
                    Some(quality_score_val.score)
                };
                
                Ok(Some(voidm_db::models::Memory {
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
                    title,
                }))
            }
        }
    }

    /// List memories (internal implementation, extracted from voidm-core)
    async fn list_memories_impl(&self, scope_filter: Option<&str>, type_filter: Option<&str>, limit: usize) -> Result<Vec<voidm_db::models::Memory>> {
        // Build query dynamically
        let rows: Vec<(String, String, String, i64, String, String, Option<f32>, Option<String>, String, String)> = match (scope_filter, type_filter) {
            (Some(scope), Some(type_filter)) => {
                let scope_prefix = format!("{}%", scope);
                sqlx::query_as(
                    "SELECT DISTINCT m.id, m.type, m.content, m.importance, m.tags, m.metadata, m.quality_score, m.context, m.created_at, m.updated_at
                     FROM memories m
                     JOIN edges es ON es.from_id = m.id AND es.edge_type = 'HAS_SCOPE'
                     JOIN nodes sn ON sn.id = es.to_id AND sn.type = 'Scope'
                     JOIN edges e ON e.from_id = m.id AND e.edge_type = 'HAS_TYPE'
                     JOIN nodes n ON n.id = e.to_id AND n.type = 'MemoryType'
                     WHERE json_extract(sn.properties, '$.name') LIKE ? AND json_extract(n.properties, '$.name') = ?
                     ORDER BY m.created_at DESC LIMIT ?"
                )
                .bind(&scope_prefix)
                .bind(type_filter)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(scope), None) => {
                let scope_prefix = format!("{}%", scope);
                sqlx::query_as(
                    "SELECT DISTINCT m.id, m.type, m.content, m.importance, m.tags, m.metadata, m.quality_score, m.context, m.created_at, m.updated_at
                     FROM memories m
                     JOIN edges es ON es.from_id = m.id AND es.edge_type = 'HAS_SCOPE'
                     JOIN nodes sn ON sn.id = es.to_id AND sn.type = 'Scope'
                     WHERE json_extract(sn.properties, '$.name') LIKE ?
                     ORDER BY m.created_at DESC LIMIT ?"
                )
                .bind(&scope_prefix)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(type_filter)) => {
                sqlx::query_as(
                    "SELECT DISTINCT m.id, m.type, m.content, m.importance, m.tags, m.metadata, m.quality_score, m.context, m.created_at, m.updated_at
                     FROM memories m
                     JOIN edges e ON e.from_id = m.id AND e.edge_type = 'HAS_TYPE'
                     JOIN nodes n ON n.id = e.to_id AND n.type = 'MemoryType'
                     WHERE json_extract(n.properties, '$.name') = ?
                     ORDER BY m.created_at DESC LIMIT ?"
                )
                .bind(type_filter)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None) => {
                sqlx::query_as(
                    "SELECT id, type, content, importance, tags, metadata, quality_score, context, created_at, updated_at
                     FROM memories ORDER BY created_at DESC LIMIT ?"
                )
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?
            }
        };

        let mut memories = Vec::new();
        for (id, memory_type, content, importance, tags_json, metadata_json, quality_score_db, context, created_at, updated_at) in rows {
            let scopes = utils::get_scopes(&self.pool, &id).await?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap_or_default();
            
            // Use persisted quality_score if available, otherwise compute
            let quality_score = if let Some(score) = quality_score_db {
                Some(score)
            } else {
                let memory_type_enum: voidm_db::models::MemoryType = memory_type.parse().unwrap_or(voidm_db::models::MemoryType::Semantic);
                let quality_mt = voidm_core::crud::convert_memory_type(&memory_type_enum);
                let quality_score_val = voidm_scoring::compute_quality_score(&content, &quality_mt);
                Some(quality_score_val.score)
            };
            
            memories.push(voidm_db::models::Memory { 
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
            });
        }
        Ok(memories)
    }
    /// Extracted from voidm-core::crud::delete_memory to keep sqlx in backend only
    async fn delete_memory_impl(&self, id: &str) -> Result<bool> {
        // Resolve short IDs to full IDs (supports 4+ char prefixes)
        let full_id = utils::resolve_id_sqlite(&self.pool, id).await
            .or_else(|_| {
                // If resolution fails, treat as not found
                Ok::<String, anyhow::Error>(String::new())
            })?;
        
        if full_id.is_empty() {
            return Ok(false);
        }

        // Check if memory exists first
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?)")
            .bind(&full_id)
            .fetch_one(&self.pool)
            .await?;
        
        if !exists {
            return Ok(false);
        }

        let chunk_ids: Vec<String> = sqlx::query_scalar("SELECT to_id FROM edges WHERE from_id = ? AND edge_type = 'HAS_CHUNK'")
            .bind(&full_id)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        for chunk_id in chunk_ids {
            sqlx::query("DELETE FROM edges WHERE from_id = ? OR to_id = ?")
                .bind(&chunk_id)
                .bind(&chunk_id)
                .execute(&self.pool)
                .await?;
            sqlx::query("DELETE FROM nodes WHERE id = ?")
                .bind(&chunk_id)
                .execute(&self.pool)
                .await?;
        }

        sqlx::query("DELETE FROM edges WHERE from_id = ? OR to_id = ?")
            .bind(&full_id)
            .bind(&full_id)
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM nodes WHERE id = ?")
            .bind(&full_id)
            .execute(&self.pool)
            .await?;


        let _ = sqlx::query("DELETE FROM vec_memories WHERE memory_id = ?")
            .bind(&full_id)
            .execute(&self.pool)
            .await;

        sqlx::query("DELETE FROM memories_fts WHERE id = ?")
            .bind(&full_id)
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(&full_id)
            .execute(&self.pool)
            .await?;

        Ok(true)
    }

    // TODO: Phase 1.5 - Create add_memory_impl here with 16+ sqlx calls from voidm-core
}

impl Database for SqliteDatabase {
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            sqlx::query("SELECT 1").execute(&pool).await?;
            Ok(())
        })
    }

    fn close(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            pool.close().await;
            Ok(())
        })
    }

    fn ensure_schema(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            // Use migrate::run to run the complete schema
            // This ensures all tables and migrations are applied properly
            migrate::run(&pool).await?;
            Ok(())
        })
    }

    fn add_memory(
        &self,
        req_json: Value,
        config_json: &Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>> {
        let pool = self.pool.clone();
        let config_json = config_json.clone();
        Box::pin(async move {
            // Deserialize request and config
            let req: voidm_db::models::AddMemoryRequest = serde_json::from_value(req_json)
                .context("Failed to parse AddMemoryRequest")?;
            let config: voidm_core::Config = serde_json::from_value(config_json)
                .context("Failed to parse Config")?;
            
            // Prepare pre-transaction data
            let pre_tx_data = add_memory_backend::prepare_add_memory_data(&pool, req, &config).await?;
            
            // Execute transaction with prepared data
            let resp = add_memory_backend::execute_add_memory_transaction_wrapper(&pool, pre_tx_data).await?;
            
            // Serialize response with proper field name "type" (not "memory_type")
            Ok(serde_json::json!({
                "id": resp.id,
                "type": resp.memory_type,
                "content": resp.content,
                "scopes": resp.scopes,
                "tags": resp.tags,
                "importance": resp.importance,
                "created_at": resp.created_at,
                "quality_score": resp.quality_score,
                "context": resp.context,
                "title": resp.title,
                "suggested_links": resp.suggested_links,
                "duplicate_warning": resp.duplicate_warning,
            }))
        })
    }

    fn get_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<Value>>> + Send + '_>> {
        let id = id.to_string();
        Box::pin(async move {
            match self.get_memory_impl(&id).await {
                Ok(Some(mem)) => Ok(Some(serde_json::to_value(mem)?)),
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            }
        })
    }

    fn list_memories(
        &self,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        Box::pin(async move {
            match self.list_memories_impl(None, None, limit.unwrap_or(100)).await {
                Ok(mems) => Ok(mems.into_iter().map(|m| serde_json::to_value(m).unwrap_or(Value::Null)).collect()),
                Err(e) => Err(e),
            }
        })
    }

    fn delete_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let id = id.to_string();
        Box::pin(async move {
            self.delete_memory_impl(&id).await
        })
    }

    fn update_memory(
        &self,
        id: &str,
        content: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let content = content.to_string();
        Box::pin(async move {
            let resolved_id = crate::utils::resolve_id_sqlite(&pool, &id).await.unwrap_or(id.clone());
            let now = Utc::now().to_rfc3339();
            sqlx::query("UPDATE memories SET content = ?, updated_at = ? WHERE id = ?")
                .bind(&content)
                .bind(&now)
                .bind(&resolved_id)
                .execute(&pool)
                .await
                .context("Failed to update memory")?;

            let title: Option<String> = sqlx::query_scalar("SELECT title FROM memories WHERE id = ?")
                .bind(&resolved_id)
                .fetch_one(&pool)
                .await?;

            sqlx::query("DELETE FROM memories_fts WHERE id = ?")
                .bind(&resolved_id)
                .execute(&pool)
                .await?;
            sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES (?, ?, ?)")
                .bind(&resolved_id)
                .bind(title.as_deref().unwrap_or(""))
                .bind(&content)
                .execute(&pool)
                .await?;

            let chunk_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM memory_chunks WHERE memory_id = ?")
                .bind(&resolved_id)
                .fetch_all(&pool)
                .await?;
            sqlx::query("DELETE FROM memory_chunks WHERE memory_id = ?")
                .bind(&resolved_id)
                .execute(&pool)
                .await?;
            sqlx::query("DELETE FROM edges WHERE from_id = ? AND edge_type = 'HAS_CHUNK'")
                .bind(&resolved_id)
                .execute(&pool)
                .await?;
            for chunk_id in chunk_ids {
                sqlx::query("DELETE FROM edges WHERE from_id = ? OR to_id = ?")
                    .bind(&chunk_id)
                    .bind(&chunk_id)
                    .execute(&pool)
                    .await?;
                sqlx::query("DELETE FROM nodes WHERE id = ?")
                    .bind(&chunk_id)
                    .execute(&pool)
                    .await?;
            }

            let chunk_config = voidm_embeddings::ChunkingConfig {
                target_size: voidm_core::memory_policy::CHUNK_TARGET_SIZE,
                min_chunk_size: voidm_core::memory_policy::CHUNK_MIN_SIZE,
                max_chunk_size: voidm_core::memory_policy::CHUNK_MAX_SIZE,
                overlap: voidm_core::memory_policy::CHUNK_OVERLAP,
                smart_breaks: true,
            };
            let chunks = voidm_embeddings::chunk_memory(&resolved_id, &content, &now, &chunk_config);
            let chunk_texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
            let model_name = sqlx::query_scalar::<_, String>("SELECT value FROM db_meta WHERE key = 'embedding_model'")
                .fetch_optional(&pool)
                .await?
                .unwrap_or_else(|| "all-MiniLM-L6-v2".to_string());
            let chunk_embeddings = match voidm_embeddings::embed_batch(&model_name, &chunk_texts) {
                Ok(embs) => embs,
                Err(_) => Vec::new(),
            };

            for (idx, chunk) in chunks.into_iter().enumerate() {
                let maybe_embedding = chunk_embeddings.get(idx);
                let embedding_bytes = maybe_embedding.map(|emb| {
                    emb.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>()
                });
                let embedding_dim = maybe_embedding.map(|emb| emb.len() as i64);

                sqlx::query(
                    "INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at, embedding, embedding_dim)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(&chunk.id)
                .bind(&resolved_id)
                .bind(&chunk.content)
                .bind(chunk.index as i64)
                .bind(chunk.size as i64)
                .bind(format!("{:?}", chunk.break_type))
                .bind(&chunk.created_at)
                .bind(embedding_bytes)
                .bind(embedding_dim)
                .execute(&pool)
                .await?;

                sqlx::query(
                    "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'MemoryChunk', ?, ?, ?)"
                )
                    .bind(&chunk.id)
                    .bind(serde_json::json!({
                        "memory_id": resolved_id,
                        "index": chunk.index,
                        "size": chunk.size,
                        "break_type": format!("{:?}", chunk.break_type),
                        "content": chunk.content,
                    }).to_string())
                    .bind(&chunk.created_at)
                    .bind(&chunk.created_at)
                    .execute(&pool)
                    .await?;
                sqlx::query(
                    "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, 'HAS_CHUNK', ?, ?, ?)"
                )
                    .bind(format!("{}:HAS_CHUNK:{}", resolved_id, chunk.id))
                    .bind(&resolved_id)
                    .bind(&chunk.id)
                    .bind(serde_json::json!({"sequence_num": chunk.index}).to_string())
                    .bind(&chunk.created_at)
                    .execute(&pool)
                    .await?;
            }
            Ok(())
        })
    }

    fn resolve_memory_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<voidm_db::ResolveResult>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        
        Box::pin(async move {
            // Try exact match first
            if let Ok(Some(row)) = sqlx::query_scalar::<_, String>(
                "SELECT id FROM memories WHERE id = ?"
            )
                .bind(&id)
                .fetch_optional(&pool)
                .await
            {
                return Ok(voidm_db::ResolveResult::Single(row));
            }
            
            // Prefix match requires min 8 chars
            if id.len() < 8 {
                anyhow::bail!("Memory ID prefix '{}' is too short (minimum 8 characters)", id);
            }
            
            // Fetch all matching prefixes
            let matches = sqlx::query_scalar::<_, String>(
                "SELECT id FROM memories WHERE id LIKE ? ORDER BY id"
            )
                .bind(format!("{}%", id))
                .fetch_all(&pool)
                .await
                .context("Failed to resolve ID")?;
            
            match matches.len() {
                0 => anyhow::bail!("Memory '{}' not found", id),
                1 => Ok(voidm_db::ResolveResult::Single(matches.into_iter().next().unwrap())),
                _ => Ok(voidm_db::ResolveResult::Multiple(matches)),  // Return all for bulk delete
            }
        })
    }

    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows = sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT scope FROM (SELECT json_each.value as scope FROM memories, json_each(memories.scopes))"
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            Ok(rows)
        })
    }

    // ===== Memory Edges =====

    fn link_memories(
        &self,
        from_id: &str,
        rel: &str,
        to_id: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let rel = rel.to_string();
        let to_id = to_id.to_string();
        let note = note.map(|s| s.to_string());
        let now = Utc::now().to_rfc3339();

        Box::pin(async move {
            sqlx::query("INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Memory', '{}', ?, ?)")
                .bind(&from_id)
                .bind(&now)
                .bind(&now)
                .execute(&pool)
                .await
                .context("Failed to ensure source memory node")?;
            sqlx::query("INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Memory', '{}', ?, ?)")
                .bind(&to_id)
                .bind(&now)
                .bind(&now)
                .execute(&pool)
                .await
                .context("Failed to ensure target memory node")?;
            let props = note.as_ref().map(|n| serde_json::json!({"note": n})).unwrap_or_else(|| serde_json::json!({}));
            let edge_id = format!("{}:{}:{}", from_id, rel, to_id);
            sqlx::query(
                "INSERT OR REPLACE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&edge_id)
            .bind(&from_id)
            .bind(&rel)
            .bind(&to_id)
            .bind(props.to_string())
            .bind(&now)
            .execute(&pool)
            .await
            .context("Failed to link memories")?;

            let response = serde_json::json!({
                "created": true,
                "from": from_id,
                "rel": rel,
                "to": to_id,
            });
            Ok(response)
        })
    }

    fn unlink_memories(
        &self,
        from_id: &str,
        rel: &str,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let rel = rel.to_string();
        let to_id = to_id.to_string();

        Box::pin(async move {
            let result = sqlx::query("DELETE FROM edges WHERE from_id = ? AND edge_type = ? AND to_id = ?")
                .bind(&from_id)
                .bind(&rel)
                .bind(&to_id)
                .execute(&pool)
                .await
                .context("Failed to unlink memories")?;

            Ok(result.rows_affected() > 0)
        })
    }

    fn list_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows = sqlx::query("SELECT id, from_id, edge_type, to_id, properties FROM edges WHERE edge_type NOT IN ('HAS_TYPE', 'HAS_CHUNK', 'HAS_TAG')")
                .fetch_all(&pool)
                .await
                .context("Failed to list edges")?;

            Ok(rows.iter().map(|r| {
                let props: serde_json::Value = serde_json::from_str(&r.get::<String, _>("properties")).unwrap_or_else(|_| json!({}));
                json!({
                    "id": r.get::<String, _>("id"),
                    "from_id": r.get::<String, _>("from_id"),
                    "rel_type": r.get::<String, _>("edge_type"),
                    "to_id": r.get::<String, _>("to_id"),
                    "note": props.get("note").cloned().unwrap_or(serde_json::Value::Null)
                })
            }).collect())
        })
    }

    // ===== Search Stubs (for now) =====

    fn search_bm25(
        &self,
        query: &str,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let pool = self.pool.clone();
        let query_str = query.to_string();
        let _scope_filter = scope_filter.map(|s| s.to_string());
        let _type_filter = type_filter.map(|s| s.to_string());

        Box::pin(async move {
            // Query FTS5 using raw query to avoid type serialization issues
            let fts_sql = "
                SELECT id, bm25(memories_fts) as bm25_score
                FROM memories_fts
                WHERE memories_fts MATCH ?
                ORDER BY bm25_score DESC
                LIMIT ?
            ";

            // Use raw query to manually deserialize
            let rows = sqlx::query(fts_sql)
                .bind(&query_str)
                .bind((limit * 2) as i64)
                .fetch_all(&pool)
                .await?;

            let mut results: Vec<(String, f32)> = Vec::new();
            
            for row in rows {
                let id: String = row.try_get("id")?;
                let score: f64 = row.try_get::<f64, _>("bm25_score")?;
                
                // Normalize BM25 score: negative → [0, 1]
                // BM25 returns scores in range roughly [-10, 0]
                // Use exp(score) to convert to [0, 1]
                let normalized = (score as f32).exp();
                let clamped = normalized.clamp(0.0, 1.0);
                
                results.push((id, clamped));
                
                if results.len() >= limit {
                    break;
                }
            }

            Ok(results)
        })
    }

    fn search_fuzzy(
        &self,
        _query: &str,
        _scope_filter: Option<&str>,
        _limit: usize,
        _threshold: f32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        Box::pin(async move {
            Ok(vec![])
        })
    }

    fn search_title_bm25(
        &self,
        query: &str,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let pool = self.pool.clone();
        let query = query.to_string();
        let scope_filter = scope_filter.map(|s| s.to_string());
        let type_filter = type_filter.map(|s| s.to_string());
        Box::pin(async move {
            let mut sql = String::from(
                "SELECT DISTINCT f.id, bm25(memories_fts, 10.0, 0.1) as bm25_score FROM memories_fts f JOIN memories m ON m.id = f.id"
            );

            if type_filter.is_some() {
                sql.push_str(" JOIN edges e ON e.from_id = m.id AND e.edge_type = 'HAS_TYPE' JOIN nodes n ON n.id = e.to_id AND n.type = 'MemoryType'");
            }

            sql.push_str(" WHERE f.title MATCH ?");

            if scope_filter.is_some() {
                sql.push_str(" AND EXISTS (SELECT 1 FROM edges es JOIN nodes sn ON sn.id = es.to_id AND sn.type = 'Scope' WHERE es.from_id = m.id AND es.edge_type = 'HAS_SCOPE' AND json_extract(sn.properties, '$.name') LIKE ? || '%')");
            }
            if type_filter.is_some() {
                sql.push_str(" AND json_extract(n.properties, '$.name') = ?");
            }

            sql.push_str(" ORDER BY bm25_score LIMIT ?");

            let mut q = sqlx::query(&sql).bind(&query);
            if let Some(scope) = &scope_filter {
                q = q.bind(scope);
            }
            if let Some(type_filter) = &type_filter {
                q = q.bind(type_filter);
            }
            let rows = q
            .bind((limit * 2) as i64)
            .fetch_all(&pool)
            .await?;

            let mut results = Vec::new();
            for row in rows {
                let id: String = row.try_get("id")?;
                let score: f64 = row.try_get::<f64, _>("bm25_score")?;
                let normalized = (score as f32).exp().clamp(0.0, 1.0);
                results.push((id, normalized));
                if results.len() >= limit {
                    break;
                }
            }
            Ok(results)
        })
    }

    fn search_ann(
        &self,
        embedding: Vec<f32>,
        limit: usize,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let pool = self.pool.clone();
        let scope_filter = scope_filter.map(|s| s.to_string());
        let type_filter = type_filter.map(|s| s.to_string());

        Box::pin(async move {
            // For now, use a simpler approach: query all memories with embeddings 
            // and compute similarity client-side
            // TODO: Replace with proper sqlite-vec ANN when extension is stable
            
            let mut sql = r#"
                SELECT m.id, m.importance, v.embedding
                FROM vec_memories v
                JOIN memories m ON v.memory_id = m.id
                WHERE 1=1
            "#.to_string();

            if type_filter.is_some() {
                sql.push_str(" AND m.type = ?");
            }

            if scope_filter.is_some() {
                sql.push_str(" AND EXISTS (SELECT 1 FROM edges es JOIN nodes sn ON sn.id = es.to_id AND sn.type = 'Scope' WHERE es.from_id = m.id AND es.edge_type = 'HAS_SCOPE' AND json_extract(sn.properties, '$.name') LIKE ? || '%')");
            }

            sql.push_str(&format!(" LIMIT {}", limit * 2)); // Over-fetch for reranking

            let mut query_builder = sqlx::query_as::<_, (String, i64, Vec<u8>)>(&sql);

            if let Some(ref type_) = type_filter {
                query_builder = query_builder.bind(type_);
            }

            if let Some(ref scope) = scope_filter {
                query_builder = query_builder.bind(scope);
            }

            let rows = query_builder.fetch_all(&pool).await.unwrap_or_default();

            // Compute cosine similarity for each row
            let mut scored_results: Vec<(String, f32)> = rows.into_iter().filter_map(|(id, _importance, embedding_bytes)| {
                // Convert bytes back to f32 vector
                if embedding_bytes.len() % 4 != 0 {
                    return None;
                }
                
                let mut stored_vec = Vec::with_capacity(embedding_bytes.len() / 4);
                for chunk in embedding_bytes.chunks(4) {
                    if let Ok(bytes) = <[u8; 4]>::try_from(chunk) {
                        stored_vec.push(f32::from_le_bytes(bytes));
                    }
                }

                // Compute cosine similarity
                if stored_vec.len() != embedding.len() {
                    return None;
                }

                let dot_product: f32 = embedding.iter().zip(&stored_vec).map(|(a, b)| a * b).sum();
                let query_norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                let stored_norm: f32 = stored_vec.iter().map(|x| x * x).sum::<f32>().sqrt();

                if query_norm < 1e-10 || stored_norm < 1e-10 {
                    return Some((id, 0.0));
                }

                let similarity = dot_product / (query_norm * stored_norm);
                let normalized = (similarity + 1.0) / 2.0; // Convert [-1, 1] to [0, 1]

                Some((id, normalized.clamp(0.0, 1.0)))
            }).collect();

            // Sort by similarity descending
            scored_results.sort_by(|a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            });

            scored_results.truncate(limit);
            Ok(scored_results)
        })
    }

    fn fetch_memories_raw(
        &self,
        _scope_filter: Option<&str>,
        _type_filter: Option<&str>,
        _limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>>> + Send + '_>> {
        Box::pin(async move {
            Ok(vec![])
        })
    }

    fn fetch_memories_for_chunking(
        &self,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let memories: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT id, content, created_at FROM memories \
                 ORDER BY created_at DESC \
                 LIMIT ?"
            )
            .bind(limit as i32)
            .fetch_all(&pool)
            .await?;
            Ok(memories)
        })
    }

    fn search_chunk_ann(
        &self,
        embedding: Vec<f32>,
        limit: usize,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let pool = self.pool.clone();
        let scope_filter = scope_filter.map(|s| s.to_string());
        let type_filter = type_filter.map(|s| s.to_string());
        Box::pin(async move {
            let dim = embedding.len() as i32;
            let mut sql = String::from(
                "SELECT mc.id, mc.embedding, mc.embedding_dim
                 FROM memory_chunks mc
                 JOIN memories m ON m.id = mc.memory_id"
            );

            if type_filter.is_some() {
                sql.push_str(" JOIN edges e ON e.from_id = m.id AND e.edge_type = 'HAS_TYPE' JOIN nodes n ON n.id = e.to_id AND n.type = 'MemoryType'");
            }

            sql.push_str(" WHERE mc.embedding IS NOT NULL AND mc.embedding_dim = ?");
            if let Some(scope) = &scope_filter {
                let _ = scope;
                sql.push_str(" AND EXISTS (SELECT 1 FROM edges es JOIN nodes sn ON sn.id = es.to_id AND sn.type = 'Scope' WHERE es.from_id = m.id AND es.edge_type = 'HAS_SCOPE' AND json_extract(sn.properties, '$.name') LIKE ? || '%')");
            }
            if let Some(type_filter) = &type_filter {
                let _ = type_filter;
                sql.push_str(" AND json_extract(n.properties, '$.name') = ?");
            }

            let mut q = sqlx::query_as::<_, (String, Vec<u8>, i32)>(&sql).bind(dim);
            if let Some(scope) = &scope_filter {
                q = q.bind(scope);
            }
            if let Some(type_filter) = &type_filter {
                q = q.bind(type_filter);
            }

            let chunks: Vec<(String, Vec<u8>, i32)> = q
            .fetch_all(&pool)
            .await?;

            let mut similarities = Vec::new();
            for (chunk_id, embedding_bytes, d) in chunks {
                if d != dim { continue; }
                let mut stored = Vec::new();
                for chunk in embedding_bytes.chunks(4) {
                    if let Ok(bytes) = <[u8; 4]>::try_from(chunk) {
                        stored.push(f32::from_le_bytes(bytes));
                    }
                }
                if stored.len() == embedding.len() {
                    let sim = voidm_core::fast_vector::cosine_similarity(&embedding, &stored);
                    similarities.push((chunk_id, sim));
                }
            }

            similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            similarities.truncate(limit);
            Ok(similarities)
        })
    }

    fn search_hybrid(
        &self,
        _opts: Value,
        _model_name: &str,
        _embeddings_enabled: bool,
        _config_min_score: f32,
        _config_search: &Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>> {
        Box::pin(async move {
            Ok(json!({"results": []}))
        })
    }

    // ===== Ontology =====


    // ===== Graph Stubs =====

    fn query_cypher(&self, _query: &str, _params: &Value) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>> {
        Box::pin(async move {
            anyhow::bail!("Cypher not supported on SQLite backend")
        })
    }

    fn get_neighbors(&self, _id: &str, _depth: usize) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>> {
        Box::pin(async move {
            anyhow::bail!("Graph traversal not fully supported on SQLite backend")
        })
    }

    fn get_statistics(&self) -> Pin<Box<dyn Future<Output = Result<voidm_db::models::DatabaseStats>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            use std::collections::HashMap;
            
            // Memory counts total + by type
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
                .fetch_one(&pool).await?;

            let by_type: Vec<(String, i64)> = sqlx::query_as(
                "SELECT type, COUNT(*) FROM memories GROUP BY type ORDER BY COUNT(*) DESC"
            ).fetch_all(&pool).await?;

            // Scope count from canonical graph truth
            let scope_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(DISTINCT json_extract(n.properties, '$.name'))
                 FROM edges e
                 JOIN nodes n ON n.id = e.to_id
                 WHERE e.edge_type = 'HAS_SCOPE' AND n.type = 'Scope'"
            ).fetch_one(&pool).await?;

            // Tag counts (top 10)
            let all_tags: Vec<(String,)> = sqlx::query_as(
                "SELECT tags FROM memories WHERE tags != '[]'"
            ).fetch_all(&pool).await?;

            let mut tag_counts: HashMap<String, usize> = HashMap::new();
            for (tags_json,) in &all_tags {
                let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
                for tag in tags {
                    *tag_counts.entry(tag).or_default() += 1;
                }
            }
            let mut top_tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
            top_tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            top_tags.truncate(10);

            // Graph counts
            let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
                .fetch_one(&pool).await.unwrap_or(0);
            let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges")
                .fetch_one(&pool).await.unwrap_or(0);

            let edge_by_type: Vec<(String, i64)> = sqlx::query_as(
                "SELECT edge_type, COUNT(*) FROM edges GROUP BY edge_type ORDER BY COUNT(*) DESC"
            ).fetch_all(&pool).await.unwrap_or_default();

            // Embedding coverage
            let vec_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM vec_memories"
            ).fetch_one(&pool).await.unwrap_or(0);
            
            let coverage_pct = if total > 0 {
                (vec_count as f64 / total as f64) * 100.0
            } else {
                0.0
            };

            Ok(voidm_db::models::DatabaseStats {
                total_memories: total,
                memories_by_type: by_type,
                scopes_count: scope_count,
                top_tags,
                embedding_coverage: voidm_db::models::EmbeddingStats {
                    total_embeddings: vec_count,
                    total_memories: total,
                    coverage_percentage: coverage_pct,
                },
                graph: voidm_db::models::GraphStats {
                    node_count,
                    edge_count,
                    edges_by_type: edge_by_type,
                },
                db_size_bytes: 0, // Set by caller if needed
            })
        })
    }

    fn get_graph_stats(&self) -> Pin<Box<dyn Future<Output = Result<voidm_db::models::GraphStats>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
                .fetch_one(&pool).await.unwrap_or(0);
            let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges")
                .fetch_one(&pool).await.unwrap_or(0);

            let edges_by_type: Vec<(String, i64)> = sqlx::query_as(
                "SELECT edge_type, COUNT(*) FROM edges GROUP BY edge_type ORDER BY COUNT(*) DESC"
            ).fetch_all(&pool).await.unwrap_or_default();

            Ok(voidm_db::models::GraphStats {
                node_count,
                edge_count,
                edges_by_type,
            })
        })
    }

    fn get_graph_export_data(&self) -> Pin<Box<dyn Future<Output = Result<voidm_db::models::GraphExportData>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            // Get all memories
            let memories: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT id, type, SUBSTR(content, 1, 50) as preview FROM memories LIMIT 1000"
            )
            .fetch_all(&pool)
            .await?;

            // Get all concepts
            let concepts: Vec<(String, String)> = sqlx::query_as(
                "SELECT id, name FROM ontology_concepts LIMIT 500"
            )
            .fetch_all(&pool)
            .await?;

            let generic_nodes: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT id, type, properties FROM nodes LIMIT 2000"
            )
            .fetch_all(&pool)
            .await?;

            let generic_edges: Vec<(String, String, String, String)> = sqlx::query_as(
                "SELECT from_id, to_id, edge_type, properties FROM edges LIMIT 2000"
            )
            .fetch_all(&pool)
            .await?;

            Ok(voidm_db::models::GraphExportData {
                memories: memories.into_iter()
                    .map(|(id, mem_type, preview)| voidm_db::models::GraphMemory { id, mem_type, preview })
                    .collect(),
                concepts: concepts.into_iter()
                    .map(|(id, name)| voidm_db::models::GraphConcept { id, name })
                    .collect(),
                nodes: generic_nodes.into_iter()
                    .map(|(id, node_type, properties)| voidm_db::models::GenericGraphNode {
                        id,
                        node_type,
                        properties: serde_json::from_str(&properties).unwrap_or(serde_json::Value::Null),
                    })
                    .collect(),
                edges: generic_edges.into_iter()
                    .map(|(from_id, to_id, rel_type, properties)| voidm_db::models::GraphEdge {
                        from_id,
                        to_id,
                        rel_type,
                        properties: serde_json::from_str(&properties).unwrap_or(serde_json::Value::Null),
                    })
                    .collect(),
            })
        })
    }

    fn check_model_mismatch(&self, _configured_model: &str) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>> {
        Box::pin(async move {
            Ok(None)
        })
    }

    fn shutdown(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            // Synchronous flush for SQLite
            sqlx::query("PRAGMA synchronous = FULL")
                .execute(&pool)
                .await?;
            // WAL checkpoint to ensure durability
            sqlx::query("PRAGMA wal_checkpoint(RESTART)")
                .execute(&pool)
                .await?;
            Ok(())
        })
    }

    fn delete_chunks_for_memory(
        &self,
        memory_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        let pool = self.pool.clone();
        let memory_id = memory_id.to_string();
        Box::pin(async move {
            let result = sqlx::query("DELETE FROM memory_chunks WHERE memory_id = ?")
                .bind(&memory_id)
                .execute(&pool)
                .await?;
            sqlx::query("DELETE FROM edges WHERE from_id = ? AND edge_type = 'HAS_CHUNK'")
                .bind(&memory_id)
                .execute(&pool)
                .await?;
            sqlx::query("DELETE FROM nodes WHERE id IN (SELECT id FROM memory_chunks WHERE memory_id = ?)")
                .bind(&memory_id)
                .execute(&pool)
                .await?;
            Ok(result.rows_affected() as usize)
        })
    }

    fn fetch_chunks(
        &self,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let chunks: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT id, text, memory_id FROM memory_chunks ORDER BY created_at DESC, \"index\" ASC LIMIT ?"
            )
            .bind(limit as i64)
            .fetch_all(&pool)
            .await?;
            Ok(chunks)
        })
    }

    fn upsert_chunk(
        &self,
        chunk_id: &str,
        memory_id: &str,
        content: &str,
        index: usize,
        size: usize,
        created_at: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        let chunk_id = chunk_id.to_string();
        let memory_id = memory_id.to_string();
        let content = content.to_string();
        let index = index as i64;
        let size = size as i64;
        let created_at = created_at.to_string();

        Box::pin(async move {
            sqlx::query(
                "INSERT OR REPLACE INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at, embedding, embedding_dim)
                 VALUES (?, ?, ?, ?, ?, COALESCE((SELECT break_type FROM memory_chunks WHERE id = ?), 'Migration'), ?, NULL, NULL)"
            )
            .bind(&chunk_id)
            .bind(&memory_id)
            .bind(&content)
            .bind(index)
            .bind(size)
            .bind(&chunk_id)
            .bind(&created_at)
            .execute(&pool)
            .await?;

            sqlx::query("INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'MemoryChunk', ?, ?, ?)")
                .bind(&chunk_id)
                .bind(serde_json::json!({"memory_id": memory_id, "index": index, "size": size, "content": content}).to_string())
                .bind(&created_at)
                .bind(&created_at)
                .execute(&pool)
                .await?;

            sqlx::query("INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, 'HAS_CHUNK', ?, ?, ?)")
                .bind(format!("{}:HAS_CHUNK:{}", memory_id, chunk_id))
                .bind(&memory_id)
                .bind(&chunk_id)
                .bind(serde_json::json!({"sequence_num": index}).to_string())
                .bind(&created_at)
                .execute(&pool)
                .await?;
            Ok(())
        })
    }

    fn store_chunk_embedding(
        &self,
        chunk_id: String,
        _memory_id: String,
        embedding: Vec<f32>,
    ) -> Pin<Box<dyn Future<Output = Result<(String, usize)>> + Send + '_>> {
        let pool = self.pool.clone();
        let dim = embedding.len();

        Box::pin(async move {
            // SQLite with sqlite-vector extension
            // Store embedding as f32 vector directly
            let embedding_bytes: Vec<u8> = embedding
                .iter()
                .flat_map(|f| f.to_le_bytes().to_vec())
                .collect();

            let result = sqlx::query(
                "UPDATE memory_chunks 
                 SET embedding = ?1, embedding_dim = ?2 
                 WHERE id = ?3"
            )
            .bind(&embedding_bytes)
            .bind(dim as i32)
            .bind(&chunk_id)
            .execute(&pool)
            .await;

            match result {
                Ok(_) => {
                    tracing::debug!("SQLite: Stored {}D embedding for chunk {}", dim, chunk_id);
                    Ok((chunk_id, dim))
                }
                Err(e) => {
                    tracing::warn!("SQLite: Failed to store embedding: {}", e);
                    Ok((chunk_id, 0))
                }
            }
        })
    }

    fn get_chunk_embedding(
        &self,
        chunk_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<f32>>>> + Send + '_>> {
        let pool = self.pool.clone();
        let chunk_id = chunk_id.to_string();

        Box::pin(async move {
            let result: Option<(Vec<u8>, i32)> = sqlx::query_as(
                "SELECT embedding, embedding_dim FROM memory_chunks WHERE id = ?1"
            )
            .bind(&chunk_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();

            if let Some((embedding_bytes, dim)) = result {
                let dim = dim as usize;
                let mut embedding = Vec::with_capacity(dim);
                
                for i in 0..dim {
                    let start = i * 4;
                    let end = start + 4;
                    if end <= embedding_bytes.len() {
                        let bytes = [
                            embedding_bytes[start],
                            embedding_bytes[start + 1],
                            embedding_bytes[start + 2],
                            embedding_bytes[start + 3],
                        ];
                        embedding.push(f32::from_le_bytes(bytes));
                    }
                }
                
                if embedding.len() == dim {
                    return Ok(Some(embedding));
                }
            }

            Ok(None)
        })
    }

    fn search_by_embedding(
        &self,
        query_embedding: Vec<f32>,
        limit: usize,
        min_similarity: f32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let pool = self.pool.clone();
        let dim = query_embedding.len();

        Box::pin(async move {
            // SQLite with sqlite-vector: use vector distance operations
            // Fetch all chunks with embeddings
            let chunks: Vec<(String, Vec<u8>, i32)> = sqlx::query_as(
                "SELECT id, embedding, embedding_dim FROM memory_chunks 
                 WHERE embedding IS NOT NULL AND embedding_dim = ?1
                 LIMIT 10000"
            )
            .bind(dim as i32)
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            let mut similarities = Vec::new();
            
            for (chunk_id, embedding_bytes, d) in chunks {
                let d = d as usize;
                if d != dim {
                    continue;
                }
                
                let mut embedding = Vec::with_capacity(dim);
                for i in 0..dim {
                    let start = i * 4;
                    let end = start + 4;
                    if end <= embedding_bytes.len() {
                        let bytes = [
                            embedding_bytes[start],
                            embedding_bytes[start + 1],
                            embedding_bytes[start + 2],
                            embedding_bytes[start + 3],
                        ];
                        embedding.push(f32::from_le_bytes(bytes));
                    }
                }
                
                if embedding.len() == dim {
                    if let Ok(similarity) = voidm_core::similarity::cosine_similarity(&query_embedding, &embedding) {
                        if similarity >= min_similarity {
                            similarities.push((chunk_id, similarity));
                        }
                    }
                }
            }

            // Sort by similarity descending
            similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            similarities.truncate(limit);

            tracing::debug!("SQLite: Found {} similar chunks (cosine)", similarities.len());
            Ok(similarities)
        })
    }

    fn export_to_jsonl(
        &self,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let pool = self.pool.clone();

        Box::pin(async move {
            let mut records = Vec::new();
            let limit_val = limit.unwrap_or(999999) as i64;

            // Fetch all memories with all fields
            let memories: Vec<(String, String, String, String, String, Option<String>, Option<String>, Option<String>, String)> = 
                sqlx::query_as(
                    "SELECT m.id,
                            COALESCE((
                                SELECT json_extract(n.properties, '$.name')
                                FROM edges e
                                JOIN nodes n ON n.id = e.to_id AND n.type = 'MemoryType'
                                WHERE e.from_id = m.id AND e.edge_type = 'HAS_TYPE'
                                LIMIT 1
                            ), m.type) as resolved_type,
                            m.content, m.created_at, m.updated_at, m.title, m.metadata, NULL, m.tags
                     FROM memories m
                     WHERE m.id NOT LIKE '__memorytype__:%'
                     LIMIT ?"
                )
                .bind(limit_val)
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

            for (id, mem_type, content, created_at, updated_at, title, metadata_str, scopes_str, tags_json) in memories {
                // Parse metadata from JSON string
                let metadata = metadata_str.and_then(|s| serde_json::from_str(&s).ok());
                
                // Parse scopes from JSON string
                let scopes = scopes_str.and_then(|s| serde_json::from_str(&s).ok());
                let tags = serde_json::from_str(&tags_json).ok();

                let memory_record = voidm_core::export::MemoryRecord {
                    id: id.clone(),
                    content,
                    memory_type: mem_type,
                    created_at,
                    updated_at: Some(updated_at),
                    title,
                    scope: None,
                    scopes,
                    tags,
                    metadata,
                    provenance: None,
                    context: None,
                    importance: None,
                    quality_score: None,
                };

                let record = voidm_core::export::ExportRecord::Memory(memory_record);
                if let Ok(json) = voidm_core::export::record_to_jsonl(&record) {
                    records.push(json);
                }
            }

            let chunks = self.list_chunks().await.unwrap_or_default();
            for chunk in chunks {
                if let (Some(id), Some(content)) = (
                    chunk.get("id").and_then(|v| v.as_str()),
                    chunk.get("text").and_then(|v| v.as_str()),
                ) {
                    let memory_id = sqlx::query_scalar::<_, String>(
                        "SELECT from_id FROM edges WHERE edge_type = 'HAS_CHUNK' AND to_id = ? LIMIT 1"
                    )
                    .bind(id)
                    .fetch_optional(&pool)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_default();

                    let record = voidm_core::export::ExportRecord::MemoryChunk(voidm_core::export::ChunkRecord {
                        id: id.to_string(),
                        memory_id,
                        content: content.to_string(),
                        created_at: chunk.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        coherence_score: None,
                        quality: None,
                        embedding: None,
                        embedding_dim: None,
                        embedding_model: None,
                    });
                    if let Ok(json) = voidm_core::export::record_to_jsonl(&record) {
                        records.push(json);
                    }
                }
            }

            let edges = self.list_edges().await.unwrap_or_default();
            for edge in edges {
                if let (Some(source_id), Some(rel_type), Some(target_id)) = (
                    edge.get("from_id").and_then(|v| v.as_str()),
                    edge.get("rel_type").and_then(|v| v.as_str()),
                    edge.get("to_id").and_then(|v| v.as_str()),
                ) {
                    let note = edge.get("note").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let record = voidm_core::export::ExportRecord::Relationship(voidm_core::export::RelationshipRecord {
                        source_id: source_id.to_string(),
                        rel_type: rel_type.to_string(),
                        target_id: target_id.to_string(),
                        note,
                        created_at: edge.get("created_at").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        properties: None,
                    });
                    if let Ok(json) = voidm_core::export::record_to_jsonl(&record) {
                        records.push(json);
                    }
                }
            }

            let entities = self.list_entities().await.unwrap_or_default();
            for entity in entities {
                if let (Some(id), Some(name)) = (
                    entity.get("id").and_then(|v| v.as_str()),
                    entity.get("name").and_then(|v| v.as_str()),
                ) {
                    let record = voidm_core::export::ExportRecord::Concept(voidm_core::export::ConceptRecord {
                        id: id.to_string(),
                        name: name.to_string(),
                        description: entity.get("type").and_then(|v| v.as_str()).map(|s| format!("Entity:{}", s)),
                        scope: None,
                        created_at: entity.get("created_at").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    });
                    if let Ok(json) = voidm_core::export::record_to_jsonl(&record) {
                        records.push(json);
                    }
                }
            }

            let mentions = self.list_entity_mention_edges().await.unwrap_or_default();
            for edge in mentions {
                if let (Some(source_id), Some(target_id)) = (
                    edge.get("from").and_then(|v| v.as_str()),
                    edge.get("to").and_then(|v| v.as_str()),
                ) {
                    let record = voidm_core::export::ExportRecord::Relationship(voidm_core::export::RelationshipRecord {
                        source_id: source_id.to_string(),
                        rel_type: "MENTIONS".to_string(),
                        target_id: target_id.to_string(),
                        note: None,
                        created_at: None,
                        properties: Some(serde_json::json!({
                            "confidence": edge.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5)
                        })),
                    });
                    if let Ok(json) = voidm_core::export::record_to_jsonl(&record) {
                        records.push(json);
                    }
                }
            }

            tracing::info!("SQLite: Exported {} records", records.len());
            Ok(records)
        })
    }

    fn import_from_jsonl(
        &self,
        records: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<(usize, usize, usize)>> + Send + '_>> {
        let pool = self.pool.clone();

        Box::pin(async move {
            let mut memory_count = 0;
            let mut chunk_count = 0;
            let mut relationship_count = 0;

            for line in records {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<voidm_core::export::ExportRecord>(&line) {
                    Ok(voidm_core::export::ExportRecord::Memory(mem)) => {
                        // Serialize metadata to JSON string
                        let metadata_str = mem.metadata.as_ref()
                            .and_then(|m| serde_json::to_string(m).ok())
                            .unwrap_or_else(|| "{}".to_string());

                        let tags_json = serde_json::to_string(&mem.tags.clone().unwrap_or_default())
                            .unwrap_or_else(|_| "[]".to_string());
                        let importance = mem.importance.unwrap_or(5) as i64;
                        let updated_at = mem.updated_at.as_deref().unwrap_or(&mem.created_at).to_string();

                        // Insert memory with compatibility scalar type, then materialize first-class HAS_TYPE relation.
                        let result = sqlx::query(
                            "INSERT OR IGNORE INTO memories (id, type, content, importance, tags, metadata, quality_score, context, created_at, updated_at, title)
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                        )
                        .bind(&mem.id)
                        .bind(&mem.memory_type)
                        .bind(&mem.content)
                        .bind(importance)
                        .bind(&tags_json)
                        .bind(&metadata_str)
                        .bind(mem.quality_score)
                        .bind(&mem.context)
                        .bind(&mem.created_at)
                        .bind(&updated_at)
                        .bind(&mem.title)
                        .execute(&pool)
                        .await;

                        if result.is_ok() {
                            for scope in mem.scopes.clone().unwrap_or_default() {
                                let scope_node_id = format!("__scope__:{}", scope);
                                let _ = sqlx::query(
                                    "INSERT OR IGNORE INTO memories (id, type, content, importance, tags, metadata, created_at, updated_at)
                                     VALUES (?, 'semantic', ?, 1, '[]', '{}', ?, ?)"
                                )
                                    .bind(&scope_node_id)
                                    .bind(format!("synthetic Scope carrier for {}", scope))
                                    .bind(&mem.created_at)
                                    .bind(&updated_at)
                                    .execute(&pool)
                                    .await;
                                let _ = sqlx::query(
                                    "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Scope', ?, ?, ?)"
                                )
                                    .bind(&scope_node_id)
                                    .bind(serde_json::json!({"name": scope}).to_string())
                                    .bind(&mem.created_at)
                                    .bind(&updated_at)
                                    .execute(&pool)
                                    .await;
                                let _ = sqlx::query(
                                    "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, 'HAS_SCOPE', ?, '{}', ?)"
                                )
                                    .bind(format!("{}:HAS_SCOPE:{}", mem.id, scope_node_id))
                                    .bind(&mem.id)
                                    .bind(&scope_node_id)
                                    .bind(&mem.created_at)
                                    .execute(&pool)
                                    .await;
                            }
                            let _ = sqlx::query("INSERT OR REPLACE INTO memories_fts (id, title, content) VALUES (?, ?, ?)")
                                .bind(&mem.id)
                                .bind(mem.title.as_deref().unwrap_or(""))
                                .bind(&mem.content)
                                .execute(&pool)
                                .await;

                            let synthetic_type_memory_id = format!("__memorytype__:{}", mem.memory_type);
                            let _ = sqlx::query(
                                "INSERT OR IGNORE INTO memories (id, type, content, importance, tags, metadata, created_at, updated_at)
                                 VALUES (?, 'semantic', ?, 1, '[]', '{}', ?, ?)"
                            )
                            .bind(&synthetic_type_memory_id)
                            .bind(format!("synthetic type carrier for {}", mem.memory_type))
                            .bind(&mem.created_at)
                            .bind(&updated_at)
                            .execute(&pool)
                            .await;
                            let _ = sqlx::query(
                                "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Memory', ?, ?, ?)"
                            )
                                .bind(&mem.id)
                                .bind(serde_json::json!({"title": mem.title, "context": mem.context}).to_string())
                                .bind(&mem.created_at)
                                .bind(&updated_at)
                                .execute(&pool)
                                .await;
                            let _ = sqlx::query(
                                "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'MemoryType', ?, ?, ?)"
                            )
                                .bind(&synthetic_type_memory_id)
                                .bind(serde_json::json!({"name": mem.memory_type}).to_string())
                                .bind(&mem.created_at)
                                .bind(&updated_at)
                                .execute(&pool)
                                .await;
                            let _ = sqlx::query(
                                "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, 'HAS_TYPE', ?, '{}', ?)"
                            )
                                .bind(format!("{}:HAS_TYPE:{}", mem.id, synthetic_type_memory_id))
                                .bind(&mem.id)
                                .bind(&synthetic_type_memory_id)
                                .bind(&mem.created_at)
                                .execute(&pool)
                                .await;
                            memory_count += 1;
                        }
                    }
                    Ok(voidm_core::export::ExportRecord::MemoryChunk(chunk)) => {
                        self.upsert_chunk(
                            &chunk.id,
                            &chunk.memory_id,
                            &chunk.content,
                            0,
                            chunk.content.chars().count(),
                            &chunk.created_at,
                        ).await?;
                        chunk_count += 1;
                    }
                    Ok(voidm_core::export::ExportRecord::Relationship(rel)) => {
                        if rel.rel_type == "MENTIONS" {
                            let confidence = rel.properties.as_ref()
                                .and_then(|p| p.get("confidence"))
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.5) as f32;
                            let _ = self.link_chunk_to_entity(&rel.source_id, &rel.target_id, confidence).await;
                        } else {
                            let _ = self.link_memories(&rel.source_id, &rel.rel_type, &rel.target_id, rel.note.as_deref()).await;
                        }
                        relationship_count += 1;
                    }
                    Ok(voidm_core::export::ExportRecord::Concept(concept)) => {
                        if let Some(entity_type) = concept.description.as_deref().and_then(|d| d.strip_prefix("Entity:")) {
                            let now = concept.created_at.clone().unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
                            sqlx::query(
                                "INSERT OR REPLACE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Entity', ?, ?, ?)"
                            )
                            .bind(&concept.id)
                            .bind(serde_json::json!({"name": concept.name, "entity_type": entity_type}).to_string())
                            .bind(&now)
                            .bind(&now)
                            .execute(&pool)
                            .await?;
                        }
                    }
                    Err(_) => {
                        // Skip malformed records
                        continue;
                    }
                }
            }

            tracing::info!(
                "SQLite: Imported {} memories, {} chunks, {} relationships",
                memory_count, chunk_count, relationship_count
            );
            Ok((memory_count, chunk_count, relationship_count))
        })
    }

    // ===== NEW MIGRATION METHODS (REAL IMPLEMENTATIONS) =====

    fn list_tags(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
                "SELECT id, name, created_at FROM tags"
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            let tags: Vec<Value> = rows.into_iter().map(|(id, name, created_at)| {
                json!({
                    "id": id,
                    "name": name,
                    "created_at": created_at
                })
            }).collect();

            Ok(tags)
        })
    }

    fn create_tag(&self, _name: &str) -> Pin<Box<dyn Future<Output = Result<(String, bool)>> + Send + '_>> {
        let pool = self.pool.clone();
        let name = _name.to_string();
        
        Box::pin(async move {
            let tag_id = format!("tag_{}", Uuid::new_v4());
            let now = chrono::Utc::now().to_rfc3339();

            let result = sqlx::query(
                "INSERT OR IGNORE INTO tags (id, name, created_at) VALUES (?, ?, ?)"
            )
            .bind(&tag_id)
            .bind(&name)
            .bind(&now)
            .execute(&pool)
            .await;

            match result {
                Ok(r) => {
                    let created = r.rows_affected() > 0;
                    Ok((tag_id, created))
                }
                Err(e) => Err(e.into()),
            }
        })
    }

    fn link_tag_to_memory(&self, _tag_id: &str, _memory_id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let tag_id = _tag_id.to_string();
        let memory_id = _memory_id.to_string();
        
        Box::pin(async move {
            let now = chrono::Utc::now().to_rfc3339();
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Tag', ?, ?, ?)"
            )
            .bind(&tag_id)
            .bind(serde_json::json!({}).to_string())
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await;

            let result = sqlx::query(
                "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, 'HAS_TAG', ?, '{}', ?)"
            )
            .bind(format!("{}:HAS_TAG:{}", memory_id, tag_id))
            .bind(&memory_id)
            .bind(&tag_id)
            .bind(&now)
            .execute(&pool)
            .await;

            Ok(result.is_ok())
        })
    }

    fn list_tag_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT from_id, to_id FROM edges WHERE edge_type = 'HAS_TAG'"
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            let edges: Vec<Value> = rows.into_iter().map(|(from, to)| {
                json!({
                    "from": from,
                    "to": to,
                    "type": "HAS_TAG"
                })
            }).collect();

            Ok(edges)
        })
    }

    fn list_chunks(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String, String, i64, i64, Option<String>)> = sqlx::query_as(
                "SELECT id, text, index, size, created_at FROM memory_chunks"
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            let chunks: Vec<Value> = rows.into_iter().map(|(id, text, index, size, created_at)| {
                json!({
                    "id": id,
                    "text": text,
                    "index": index,
                    "size": size,
                    "created_at": created_at
                })
            }).collect();

            Ok(chunks)
        })
    }

    fn get_chunk(&self, _chunk_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        let chunk_id = _chunk_id.to_string();
        
        Box::pin(async move {
            let row: Option<(String, String, i64, i64, Option<String>)> = sqlx::query_as(
                "SELECT id, text, index, size, created_at FROM memory_chunks WHERE id = ?"
            )
            .bind(&chunk_id)
            .fetch_optional(&pool)
            .await
            .unwrap_or(None);

            Ok(row.map(|(id, text, index, size, created_at)| {
                json!({
                    "id": id,
                    "text": text,
                    "index": index,
                    "size": size,
                    "created_at": created_at
                })
            }))
        })
    }

    fn list_chunk_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT from_id, to_id FROM edges WHERE edge_type = 'HAS_CHUNK'"
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            let edges: Vec<Value> = rows.into_iter().map(|(from, to)| {
                json!({
                    "from": from,
                    "to": to,
                    "type": "HAS_CHUNK"
                })
            }).collect();

            Ok(edges)
        })
    }

    fn list_entities(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String, String, String, String)> = sqlx::query_as(
                "SELECT id, properties, created_at, updated_at FROM nodes WHERE type = 'Entity'"
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            let entities: Vec<Value> = rows.into_iter().map(|(id, props, created_at, updated_at)| {
                let props_json = serde_json::from_str::<Value>(&props).unwrap_or_else(|_| json!({}));
                json!({
                    "id": id,
                    "name": props_json.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "type": props_json.get("entity_type").and_then(|v| v.as_str()).unwrap_or(""),
                    "created_at": created_at,
                    "updated_at": updated_at
                })
            }).collect();

            Ok(entities)
        })
    }

    fn get_or_create_entity(&self, _name: &str, _entity_type: &str) -> Pin<Box<dyn Future<Output = Result<(String, bool)>> + Send + '_>> {
        let pool = self.pool.clone();
        let name = _name.to_string();
        let entity_type = _entity_type.to_string();
        
        Box::pin(async move {
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT id FROM nodes
                 WHERE type = 'Entity'
                   AND json_extract(properties, '$.name') = ?
                   AND json_extract(properties, '$.entity_type') = ?
                 LIMIT 1"
            )
            .bind(&name)
            .bind(&entity_type)
            .fetch_optional(&pool)
            .await?;

            if let Some(entity_id) = existing {
                return Ok((entity_id, false));
            }

            let entity_id = format!("ent_{}", Uuid::new_v4());
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query(
                "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Entity', ?, ?, ?)"
            )
            .bind(&entity_id)
            .bind(json!({"name": name, "entity_type": entity_type}).to_string())
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await?;

            Ok((entity_id, true))
        })
    }

    fn link_chunk_to_entity(&self, _chunk_id: &str, _entity_id: &str, _confidence: f32) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let chunk_id = _chunk_id.to_string();
        let entity_id = _entity_id.to_string();
        let confidence = _confidence;
        
        Box::pin(async move {
            let now = chrono::Utc::now().to_rfc3339();
            let result = sqlx::query(
                "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, 'MENTIONS', ?, ?, ?)"
            )
            .bind(format!("{}:MENTIONS:{}", chunk_id, entity_id))
            .bind(&chunk_id)
            .bind(&entity_id)
            .bind(json!({"confidence": confidence}).to_string())
            .bind(&now)
            .execute(&pool)
            .await;

            Ok(result.is_ok())
        })
    }

    fn list_entity_mention_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT from_id, to_id, properties FROM edges WHERE edge_type = 'MENTIONS'"
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            let edges: Vec<Value> = rows.into_iter().map(|(from, to, props)| {
                let props_json = serde_json::from_str::<Value>(&props).unwrap_or_else(|_| json!({}));
                json!({
                    "from": from,
                    "to": to,
                    "type": "MENTIONS",
                    "confidence": props_json.get("confidence").cloned().unwrap_or(json!(null))
                })
            }).collect();

            Ok(edges)
        })
    }

    fn count_nodes(&self, _node_type: &str) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        let pool = self.pool.clone();
        let node_type = _node_type.to_string();
        
        Box::pin(async move {
            let query = match node_type.as_str() {
                "Memory" => "SELECT COUNT(*) as count FROM memories",
                "MemoryChunk" => "SELECT COUNT(*) as count FROM memory_chunks",
                "Tag" => "SELECT COUNT(*) as count FROM tags",
                "Entity" => "SELECT COUNT(*) as count FROM nodes WHERE type = 'Entity'",
                "Concept" => "SELECT COUNT(*) as count FROM concepts",
                _ => return Ok(0),
            };

            let row: (i64,) = sqlx::query_as(query)
                .fetch_optional(&pool)
                .await
                .unwrap_or(None)
                .unwrap_or((0,));

            Ok(row.0 as usize)
        })
    }

    fn count_edges(&self, _edge_type: Option<&str>) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        let pool = self.pool.clone();
        let edge_type = _edge_type.map(|s| s.to_string());
        
        Box::pin(async move {
            let query = match edge_type.as_deref() {
                Some("HAS_TAG") => "SELECT COUNT(*) as count FROM edges WHERE edge_type = 'HAS_TAG'",
                Some("HAS_CHUNK") | Some("BELONGS_TO") => "SELECT COUNT(*) as count FROM edges WHERE edge_type = 'HAS_CHUNK'",
                Some("MENTIONS") => "SELECT COUNT(*) as count FROM edges WHERE edge_type = 'MENTIONS'",
                Some(other) => {
                    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) as count FROM edges WHERE edge_type = ?")
                        .bind(other)
                        .fetch_optional(&pool)
                        .await
                        .unwrap_or(None)
                        .unwrap_or((0,));
                    return Ok(row.0 as usize);
                }
                _ => "SELECT COUNT(*) as count FROM edges",
            };

            let row: (i64,) = sqlx::query_as(query)
                .fetch_optional(&pool)
                .await
                .unwrap_or(None)
                .unwrap_or((0,));

            Ok(row.0 as usize)
        })
    }

    // ===== Generic Node/Edge API (Phase 0) =====

    fn create_node(
        &self,
        id: &str,
        node_type: &str,
        properties: Value,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let node_type = node_type.to_string();
        let props_json = properties.to_string();
        let now = Utc::now().to_rfc3339();

        Box::pin(async move {
            sqlx::query(
                "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(&node_type)
            .bind(&props_json)
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .context("Failed to create node")?;
            Ok(())
        })
    }

    fn get_node(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();

        Box::pin(async move {
            let row = sqlx::query_as::<_, (String, String, String, String, String)>(
                "SELECT id, type, properties, created_at, updated_at FROM nodes WHERE id = ?"
            )
            .bind(&id)
            .fetch_optional(&pool)
            .await
            .context("Failed to get node")?;

            Ok(row.map(|(id, node_type, props, created, updated)| {
                json!({
                    "id": id,
                    "type": node_type,
                    "properties": serde_json::from_str::<Value>(&props).unwrap_or(Value::Null),
                    "created_at": created,
                    "updated_at": updated
                })
            }))
        })
    }

    fn delete_node(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();

        Box::pin(async move {
            let result = sqlx::query("DELETE FROM nodes WHERE id = ?")
                .bind(&id)
                .execute(&pool)
                .await
                .context("Failed to delete node")?;
            Ok(result.rows_affected() > 0)
        })
    }

    fn list_nodes(&self, node_type: &str) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        let node_type = node_type.to_string();

        Box::pin(async move {
            let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
                "SELECT id, type, properties, created_at, updated_at FROM nodes WHERE type = ?"
            )
            .bind(&node_type)
            .fetch_all(&pool)
            .await
            .context("Failed to list nodes")?;

            Ok(rows.iter().map(|(id, ntype, props, created, updated)| {
                json!({
                    "id": id,
                    "type": ntype,
                    "properties": serde_json::from_str::<Value>(props).unwrap_or(Value::Null),
                    "created_at": created,
                    "updated_at": updated
                })
            }).collect())
        })
    }

    fn create_edge(
        &self,
        from_id: &str,
        edge_type: &str,
        to_id: &str,
        properties: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let edge_type = edge_type.to_string();
        let to_id = to_id.to_string();
        let id = format!("{}:{}:{}", from_id, edge_type, to_id);
        let props_json = properties.as_ref().map(|p| p.to_string()).unwrap_or_else(|| "{}".to_string());
        let now = Utc::now().to_rfc3339();

        Box::pin(async move {
            sqlx::query(
                "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(&from_id)
            .bind(&edge_type)
            .bind(&to_id)
            .bind(&props_json)
            .bind(&now)
            .execute(&pool)
            .await
            .context("Failed to create edge")?;
            Ok(())
        })
    }

    fn get_edge(
        &self,
        from_id: &str,
        edge_type: &str,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let edge_type = edge_type.to_string();
        let to_id = to_id.to_string();

        Box::pin(async move {
            let row = sqlx::query_as::<_, (String, String, String, String, String, String)>(
                "SELECT id, from_id, edge_type, to_id, properties, created_at FROM edges WHERE from_id = ? AND edge_type = ? AND to_id = ?"
            )
            .bind(&from_id)
            .bind(&edge_type)
            .bind(&to_id)
            .fetch_optional(&pool)
            .await
            .context("Failed to get edge")?;

            Ok(row.map(|(id, from, etype, to, props, created)| {
                json!({
                    "id": id,
                    "from_id": from,
                    "edge_type": etype,
                    "to_id": to,
                    "properties": serde_json::from_str::<Value>(&props).unwrap_or(Value::Null),
                    "created_at": created
                })
            }))
        })
    }

    fn delete_edge(
        &self,
        from_id: &str,
        edge_type: &str,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let edge_type = edge_type.to_string();
        let to_id = to_id.to_string();

        Box::pin(async move {
            let result = sqlx::query(
                "DELETE FROM edges WHERE from_id = ? AND edge_type = ? AND to_id = ?"
            )
            .bind(&from_id)
            .bind(&edge_type)
            .bind(&to_id)
            .execute(&pool)
            .await
            .context("Failed to delete edge")?;
            Ok(result.rows_affected() > 0)
        })
    }

    fn get_node_edges(
        &self,
        node_id: &str,
        edge_type: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        let node_id = node_id.to_string();
        let edge_type = edge_type.map(|s| s.to_string());

        Box::pin(async move {
            let query = if let Some(et) = &edge_type {
                sqlx::query_as::<_, (String, String, String, String, String, String)>(
                    "SELECT id, from_id, edge_type, to_id, properties, created_at FROM edges WHERE from_id = ? AND edge_type = ?"
                )
                .bind(&node_id)
                .bind(et)
                .fetch_all(&pool)
                .await
            } else {
                sqlx::query_as::<_, (String, String, String, String, String, String)>(
                    "SELECT id, from_id, edge_type, to_id, properties, created_at FROM edges WHERE from_id = ?"
                )
                .bind(&node_id)
                .fetch_all(&pool)
                .await
            }
            .context("Failed to get node edges")?;

            Ok(query.iter().map(|(id, from, etype, to, props, created)| {
                json!({
                    "id": id,
                    "from_id": from,
                    "edge_type": etype,
                    "to_id": to,
                    "properties": serde_json::from_str::<Value>(props).unwrap_or(Value::Null),
                    "created_at": created
                })
            }).collect())
        })
    }

    fn graph_ops(&self) -> std::sync::Arc<dyn voidm_db::graph_ops::GraphQueryOps> {
        std::sync::Arc::new(crate::graph_query_ops_impl::SqliteGraphQueryOps::new(self.pool.clone()))
    }
}

pub mod graph_query_ops_impl;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        let pool = open_pool(Path::new(":memory:")).await.unwrap();
        let db = SqliteDatabase::new(pool);
        assert!(db.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_schema_creation() {
        let pool = open_pool(Path::new(":memory:")).await.unwrap();
        let db = SqliteDatabase::new(pool);
        assert!(db.ensure_schema().await.is_ok());
    }

    #[tokio::test]
    async fn test_add_and_get_memory() {
        let pool = open_pool(Path::new(":memory:")).await.unwrap();
        let db = SqliteDatabase::new(pool);
        db.ensure_schema().await.unwrap();

        let req = json!({
            "content": "test memory",
            "type": "semantic",
            "importance": 5,
            "tags": ["test"],
            "scopes": ["global"]
        });

        let resp = db.add_memory(req, &json!({})).await.unwrap();
        let id = resp.get("id").and_then(|v| v.as_str()).unwrap();

        let mem = db.get_memory(id).await.unwrap();
        assert!(mem.is_some());
        let m = mem.unwrap();
        assert_eq!(m["content"], "test memory");
        assert_eq!(m["type"], "semantic");
    }

    #[tokio::test]
    async fn test_list_memories() {
        let pool = open_pool(Path::new(":memory:")).await.unwrap();
        let db = SqliteDatabase::new(pool);
        db.ensure_schema().await.unwrap();

        db.add_memory(
            json!({"content": "mem1", "type": "semantic"}),
            &json!({}),
        )
        .await
        .unwrap();

        db.add_memory(
            json!({"content": "mem2", "type": "episodic"}),
            &json!({}),
        )
        .await
        .unwrap();

        let list = db.list_memories(None).await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_generic_node_create_and_get() {
        let pool = open_pool(Path::new(":memory:")).await.unwrap();
        let db = SqliteDatabase::new(pool);
        db.ensure_schema().await.unwrap();

        // Create a node
        db.create_node("mem:abc123", "Memory", json!({
            "title": "Test Memory",
            "content": "Content here"
        })).await.unwrap();

        // Get the node
        let node = db.get_node("mem:abc123").await.unwrap();
        assert!(node.is_some());
        let n = node.unwrap();
        assert_eq!(n["type"], "Memory");
        assert_eq!(n["properties"]["title"], "Test Memory");
    }

    #[tokio::test]
    async fn test_generic_edge_create_and_get() {
        let pool = open_pool(Path::new(":memory:")).await.unwrap();
        let db = SqliteDatabase::new(pool);
        db.ensure_schema().await.unwrap();

        // Create nodes
        db.create_node("mem:abc", "Memory", json!({"title": "M1"})).await.unwrap();
        db.create_node("chunk:123", "Chunk", json!({"sequence_num": 0, "char_start": 0, "char_end": 100})).await.unwrap();

        // Create edge
        db.create_edge("mem:abc", "HAS_CHUNK", "chunk:123", Some(json!({"sequence_num": 0}))).await.unwrap();

        // Get edge
        let edge = db.get_edge("mem:abc", "HAS_CHUNK", "chunk:123").await.unwrap();
        assert!(edge.is_some());
        let e = edge.unwrap();
        assert_eq!(e["edge_type"], "HAS_CHUNK");
        assert_eq!(e["properties"]["sequence_num"], 0);
    }

    #[tokio::test]
    async fn test_chunk_nodes_integration() {
        use crate::chunk_nodes;

        let pool = open_pool(Path::new(":memory:")).await.unwrap();
        let db = SqliteDatabase::new(pool.clone());
        db.ensure_schema().await.unwrap();

        // Create parent memory node
        let memory_id = "mem:integration-test";
        db.create_node(memory_id, "Memory", json!({
            "title": "Test Memory",
            "content": "Lorem ipsum dolor sit amet. Consectetur adipiscing elit. Sed do eiusmod tempor."
        })).await.unwrap();

        // Simulate chunks from text
        let chunks = vec![
            "Lorem ipsum dolor sit amet.".to_string(),
            "Consectetur adipiscing elit.".to_string(),
            "Sed do eiusmod tempor.".to_string(),
        ];

        // Store chunks as nodes
        chunk_nodes::store_chunks_as_nodes(&pool, memory_id, &chunks).await.unwrap();

        // Verify chunk nodes were created
        let chunk_nodes = db.list_nodes("Chunk").await.unwrap();
        assert!(!chunk_nodes.is_empty());
        assert_eq!(chunk_nodes.len(), 3);

        // Verify first chunk has correct ordering fields
        let first_chunk = &chunk_nodes[0];
        let props = &first_chunk["properties"];
        assert!(props.get("sequence_num").is_some());
        assert!(props.get("char_start").is_some());
        assert!(props.get("char_end").is_some());
        assert!(props.get("content").is_some());

        // Verify edges exist
        let edges = db.get_node_edges(memory_id, Some("HAS_CHUNK")).await.unwrap();
        assert_eq!(edges.len(), 3);

        // Verify edges have sequence_num property
        for (i, edge) in edges.iter().enumerate() {
            assert_eq!(edge["edge_type"], "HAS_CHUNK");
            assert_eq!(edge["properties"]["sequence_num"], i);
        }
    }
}

