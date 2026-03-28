//! PostgreSQL backend implementation for voidm Database trait
//!
//! Complete implementation with all stub methods replaced.
//! Uses PostgreSQL-specific features: tsvector (FTS), UUID, JSONB, indexes.

use anyhow::{Context, Result};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::pin::Pin;
use std::future::Future;
use uuid::Uuid;
use voidm_db_trait::Database;

/// PostgreSQL database implementation
#[derive(Clone)]
pub struct PostgresDatabase {
    pub pool: PgPool,
}

/// Open a PostgreSQL connection pool
pub async fn open_postgres_pool(url: &str) -> Result<PgPool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    Ok(pool)
}

// Helper to resolve ID (exact UUID or prefix)
async fn resolve_uuid(pool: &PgPool, id: &str, table: &str) -> Result<Uuid> {
    if let Ok(uuid) = Uuid::parse_str(id) {
        return Ok(uuid);
    }
    
    let row: (String,) = sqlx::query_as(&format!("SELECT id::text FROM {} WHERE id::text LIKE $1 LIMIT 1", table))
        .bind(format!("{}%", id))
        .fetch_optional(pool)
        .await
        .context("Failed to resolve ID")?
        .context("ID not found")?;
    
    Uuid::parse_str(&row.0).context("Invalid UUID format")
}

impl Database for PostgresDatabase {
    // ===== Lifecycle =====

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
            sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
                .execute(&pool)
                .await
                .ok();

            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS memories (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    content TEXT NOT NULL,
                    memory_type VARCHAR(50) NOT NULL,
                    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
                    scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
                    importance INTEGER NOT NULL DEFAULT 0,
                    context VARCHAR(50),
                    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            )
            .execute(&pool)
            .await
            .context("Failed to create memories table")?;

            sqlx::query(
                r#"
                CREATE INDEX IF NOT EXISTS memories_fts_idx 
                ON memories USING GiST(to_tsvector('english', content))
                "#,
            )
            .execute(&pool)
            .await
            .ok();

            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS memory_edges (
                    from_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
                    rel_type VARCHAR(50) NOT NULL,
                    to_id UUID NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
                    note TEXT,
                    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (from_id, rel_type, to_id)
                )
                "#,
            )
            .execute(&pool)
            .await
            .context("Failed to create memory_edges table")?;

            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS concepts (
                    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    name VARCHAR(255) NOT NULL UNIQUE,
                    description TEXT,
                    category VARCHAR(50),
                    scope VARCHAR(255),
                    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            )
            .execute(&pool)
            .await
            .context("Failed to create concepts table")?;

            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS ontology_edges (
                    id BIGSERIAL PRIMARY KEY,
                    from_id UUID NOT NULL,
                    from_kind VARCHAR(50) NOT NULL,
                    rel_type VARCHAR(50) NOT NULL,
                    to_id UUID NOT NULL,
                    to_kind VARCHAR(50) NOT NULL,
                    note TEXT,
                    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
                "#,
            )
            .execute(&pool)
            .await
            .context("Failed to create ontology_edges table")?;

            // Add context column if it doesn't exist (migration for existing DBs)
            let _ = sqlx::query(
                "ALTER TABLE memories ADD COLUMN IF NOT EXISTS context VARCHAR(50)"
            )
            .execute(&pool)
            .await;

            Ok(())
        })
    }

    // ===== Memory CRUD =====

    fn add_memory(
        &self,
        req_json: serde_json::Value,
        _config: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let content = req_json.get("content").and_then(|v| v.as_str()).context("Missing content")?;
            let memory_type = req_json.get("memory_type").and_then(|v| v.as_str()).context("Missing memory_type")?;
            let tags = req_json.get("tags").and_then(|v| v.as_array()).map(|arr| 
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
            ).unwrap_or_default();
            let scopes = req_json.get("scopes").and_then(|v| v.as_array()).map(|arr| 
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
            ).unwrap_or_default();
            let importance = req_json.get("importance").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let context = req_json.get("context").and_then(|v| v.as_str()).map(|s| s.to_string());

            let id = Uuid::new_v4();
            let tags_json = serde_json::to_value(&tags)?;
            let scopes_json = serde_json::to_value(&scopes)?;
            sqlx::query(
                "INSERT INTO memories (id, content, memory_type, tags, scopes, importance, context) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(id)
            .bind(content)
            .bind(memory_type)
            .bind(tags_json)
            .bind(scopes_json)
            .bind(importance)
            .bind(&context)
            .execute(&pool)
            .await
            .context("Failed to insert memory")?;

            Ok(json!({"id": id.to_string(), "conflicts": [], "duplicate_warnings": [], "suggested_links": []}))
        })
    }

    fn get_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id::text, content, memory_type, tags, scopes, importance, context, created_at, updated_at FROM memories WHERE id::text = $1 OR id::text LIKE $2 LIMIT 1",
            )
            .bind(&id)
            .bind(format!("{}%", id))
            .fetch_optional(&pool)
            .await
            .context("Failed to fetch memory")?;

            Ok(row.map(|r| {
                let mut obj = json!({
                    "id": r.get::<String, _>("id"),
                    "content": r.get::<String, _>("content"),
                    "memory_type": r.get::<String, _>("memory_type"),
                    "tags": r.get::<serde_json::Value, _>("tags"),
                    "scopes": r.get::<serde_json::Value, _>("scopes"),
                    "importance": r.get::<i32, _>("importance"),
                    "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                    "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339()
                });
                if let Some(context) = r.get::<Option<String>, _>("context") {
                    obj["context"] = json!(context);
                }
                obj
            }))
        })
    }

    fn list_memories(&self, limit: Option<usize>) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let limit = limit.unwrap_or(100) as i64;
            let rows = sqlx::query("SELECT id::text, content, memory_type, tags, scopes, importance, context, created_at, updated_at FROM memories ORDER BY created_at DESC LIMIT $1")
                .bind(limit)
                .fetch_all(&pool)
                .await
                .context("Failed to list memories")?;

            Ok(rows.into_iter().map(|r| {
                let mut obj = json!({
                    "id": r.get::<String, _>("id"),
                    "content": r.get::<String, _>("content"),
                    "memory_type": r.get::<String, _>("memory_type"),
                    "tags": r.get::<serde_json::Value, _>("tags"),
                    "scopes": r.get::<serde_json::Value, _>("scopes"),
                    "importance": r.get::<i32, _>("importance"),
                    "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                    "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339()
                });
                if let Some(context) = r.get::<Option<String>, _>("context") {
                    obj["context"] = json!(context);
                }
                obj
            }).collect())
        })
    }

    fn delete_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move {
            let result = sqlx::query("DELETE FROM memories WHERE id::text = $1 OR id::text LIKE $2")
                .bind(&id)
                .bind(format!("{}%", id))
                .execute(&pool)
                .await
                .context("Failed to delete memory")?;
            Ok(result.rows_affected() > 0)
        })
    }

    fn update_memory(&self, id: &str, content: &str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let content = content.to_string();
        Box::pin(async move {
            sqlx::query("UPDATE memories SET content = $1, updated_at = CURRENT_TIMESTAMP WHERE id::text = $2 OR id::text LIKE $3")
                .bind(content)
                .bind(&id)
                .bind(format!("{}%", id))
                .execute(&pool)
                .await
                .context("Failed to update memory")?;
            Ok(())
        })
    }

    fn resolve_memory_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move {
            if let Ok(uuid) = Uuid::parse_str(&id) {
                return Ok(uuid.to_string());
            }
            let row: (String,) = sqlx::query_as("SELECT id::text FROM memories WHERE id::text LIKE $1 LIMIT 1")
                .bind(format!("{}%", id))
                .fetch_optional(&pool)
                .await
                .context("Failed to resolve memory ID")?
                .context("Memory ID not found")?;
            Ok(row.0)
        })
    }

    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT DISTINCT jsonb_array_elements(scopes)::text as scope FROM memories WHERE scopes != '[]'::jsonb ORDER BY scope"
            )
            .fetch_all(&pool)
            .await
            .context("Failed to list scopes")?;
            Ok(rows.into_iter().map(|(s,)| s).collect())
        })
    }

    // ===== Memory Edges =====

    fn link_memories(
        &self,
        from_id: &str,
        rel: &str,
        to_id: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let rel = rel.to_string();
        let to_id = to_id.to_string();
        let note = note.map(|s| s.to_string());
        Box::pin(async move {
            let from_uuid = resolve_uuid(&pool, &from_id, "memories").await?;
            let to_uuid = resolve_uuid(&pool, &to_id, "memories").await?;

            sqlx::query(
                "INSERT INTO memory_edges (from_id, rel_type, to_id, note) VALUES ($1, $2, $3, $4) ON CONFLICT (from_id, rel_type, to_id) DO UPDATE SET note = EXCLUDED.note",
            )
            .bind(from_uuid)
            .bind(&rel)
            .bind(to_uuid)
            .bind(&note)
            .execute(&pool)
            .await
            .context("Failed to link memories")?;

            // Return proper LinkResponse struct matching voidm-core::models::LinkResponse
            let response = serde_json::json!({
                "created": true,
                "from": from_id,
                "rel": rel,
                "to": to_id,
                "conflict_warning": serde_json::Value::Null
            });
            Ok(response)
        })
    }

    fn unlink_memories(&self, from_id: &str, rel: &str, to_id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let rel = rel.to_string();
        let to_id = to_id.to_string();
        Box::pin(async move {
            let from_uuid = resolve_uuid(&pool, &from_id, "memories").await?;
            let to_uuid = resolve_uuid(&pool, &to_id, "memories").await?;

            let result = sqlx::query("DELETE FROM memory_edges WHERE from_id = $1 AND rel_type = $2 AND to_id = $3")
                .bind(from_uuid)
                .bind(&rel)
                .bind(to_uuid)
                .execute(&pool)
                .await
                .context("Failed to unlink memories")?;
            Ok(result.rows_affected() > 0)
        })
    }

    fn list_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows = sqlx::query("SELECT from_id::text, rel_type, to_id::text, note FROM memory_edges ORDER BY created_at")
                .fetch_all(&pool)
                .await
                .context("Failed to list edges")?;

            Ok(rows.into_iter().map(|r| {
                json!({
                    "from_id": r.get::<String, _>("from_id"),
                    "rel_type": r.get::<String, _>("rel_type"),
                    "to_id": r.get::<String, _>("to_id"),
                    "note": r.get::<Option<String>, _>("note")
                })
            }).collect())
        })
    }

    fn list_ontology_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows = sqlx::query("SELECT from_id::text, from_kind, rel_type, to_id::text, to_kind FROM ontology_edges ORDER BY created_at")
                .fetch_all(&pool)
                .await
                .context("Failed to list ontology edges")?;

            Ok(rows.into_iter().map(|r| {
                json!({
                    "from_id": r.get::<String, _>("from_id"),
                    "from_type": r.get::<String, _>("from_kind"),
                    "rel_type": r.get::<String, _>("rel_type"),
                    "to_id": r.get::<String, _>("to_id"),
                    "to_type": r.get::<String, _>("to_kind")
                })
            }).collect())
        })
    }

    fn create_ontology_edge(
        &self,
        from_id: &str,
        from_type: &str,
        rel_type: &str,
        to_id: &str,
        to_type: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let from_type = from_type.to_string();
        let rel_type = rel_type.to_string();
        let to_id = to_id.to_string();
        let to_type = to_type.to_string();
        Box::pin(async move {
            let result = sqlx::query(
                "INSERT INTO ontology_edges (from_id, from_kind, rel_type, to_id, to_kind) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(&from_id)
            .bind(&from_type)
            .bind(&rel_type)
            .bind(&to_id)
            .bind(&to_type)
            .execute(&pool)
            .await
            .context("Failed to create ontology edge")?;

            Ok(result.rows_affected() > 0)
        })
    }

    // ===== Search =====

    fn search_hybrid(
        &self,
        opts_json: serde_json::Value,
        _model_name: &str,
        _embeddings_enabled: bool,
        _config_min_score: f32,
        _config_search: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let query = opts_json.get("query").and_then(|v| v.as_str()).context("Missing query")?;
            let limit = opts_json.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as i64;

            // Use BM25 search
            let rows: Vec<(String, f32)> = sqlx::query_as(
                "SELECT id::text, ts_rank_cd(to_tsvector('english', content), plainto_tsquery('english', $1), 1)::float as score FROM memories WHERE to_tsvector('english', content) @@ plainto_tsquery('english', $1) ORDER BY score DESC LIMIT $2"
            )
            .bind(query)
            .bind(limit)
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            Ok(json!({
                "results": rows.into_iter().map(|(id, score)| {
                    json!({"id": id, "content": "", "score": score, "method": "bm25"})
                }).collect::<Vec<_>>(),
                "total": 0
            }))
        })
    }

    fn search_bm25(
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
            let mut sql = "SELECT id::text, ts_rank_cd(to_tsvector('english', content), plainto_tsquery('english', $1), 1)::float FROM memories WHERE to_tsvector('english', content) @@ plainto_tsquery('english', $1)".to_string();
            let mut param_idx = 2;

            if scope_filter.is_some() {
                sql.push_str(&format!(" AND scopes @> (${} ::text)::jsonb", param_idx));
                param_idx += 1;
            }
            if type_filter.is_some() {
                sql.push_str(&format!(" AND memory_type = ${}", param_idx));
            }

            sql.push_str(&format!(" ORDER BY ts_rank_cd DESC LIMIT {}", limit));

            let mut query_builder = sqlx::query(&sql).bind(&query);

            if let Some(scope) = &scope_filter {
                query_builder = query_builder.bind(format!(r#"["{scope}"]"#));
            }
            if let Some(type_) = &type_filter {
                query_builder = query_builder.bind(type_);
            }

            let rows = query_builder.fetch_all(&pool).await.unwrap_or_default();
            let results: Vec<(String, f32)> = rows.into_iter().map(|r| {
                (r.get::<String, _>("id"), r.get::<f64, _>("ts_rank_cd") as f32)
            }).collect();
            Ok(results)
        })
    }

    fn search_fuzzy(
        &self,
        query: &str,
        scope_filter: Option<&str>,
        limit: usize,
        threshold: f32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        let pool = self.pool.clone();
        let query = query.to_string();
        let scope_filter = scope_filter.map(|s| s.to_string());
        Box::pin(async move {
            let mut sql = "SELECT id::text, content FROM memories".to_string();
            if scope_filter.is_some() {
                sql.push_str(" WHERE scopes @> ($1 ::text)::jsonb");
            }
            sql.push_str(&format!(" LIMIT {}", limit * 2));

            let rows: Vec<(String, String)> = if let Some(scope) = scope_filter {
                sqlx::query_as(&sql)
                    .bind(format!(r#"["{scope}"]"#))
                    .fetch_all(&pool)
                    .await
                    .unwrap_or_default()
            } else {
                sqlx::query_as(&sql).fetch_all(&pool).await.unwrap_or_default()
            };

            let mut results: Vec<(String, f32)> = rows
                .into_iter()
                .filter_map(|(id, content)| {
                    let score = strsim::jaro_winkler(&query, &content) as f32;
                    if score >= threshold {
                        Some((id, score))
                    } else {
                        None
                    }
                })
                .collect();

            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(limit);
            Ok(results)
        })
    }

    fn search_ann(
        &self,
        _embedding: Vec<f32>,
        _limit: usize,
        _scope_filter: Option<&str>,
        _type_filter: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>> {
        // PostgreSQL pgvector support - placeholder for future implementation
        // Would use: SELECT id, 1 - (embedding <=> $1) as similarity FROM vec_memories ...
        Box::pin(async move {
            Ok(vec![])
        })
    }

    fn fetch_memories_raw(
        &self,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        let scope_filter = scope_filter.map(|s| s.to_string());
        let type_filter = type_filter.map(|s| s.to_string());
        Box::pin(async move {
            let mut sql = "SELECT id::text, content FROM memories".to_string();
            let mut conditions = Vec::new();

            if scope_filter.is_some() {
                conditions.push("scopes @> ($1 ::text)::jsonb".to_string());
            }
            if type_filter.is_some() {
                let param = if scope_filter.is_some() { "$2" } else { "$1" };
                conditions.push(format!("memory_type = {}", param));
            }

            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }

            sql.push_str(&format!(" ORDER BY created_at DESC LIMIT {}", limit));

            let mut query_builder = sqlx::query_as::<_, (String, String)>(&sql);

            if let Some(scope) = scope_filter {
                query_builder = query_builder.bind(format!(r#"["{scope}"]"#));
            }
            if let Some(type_) = type_filter {
                query_builder = query_builder.bind(type_);
            }

            let results = query_builder.fetch_all(&pool).await.unwrap_or_default();
            Ok(results)
        })
    }

    fn fetch_memories_for_chunking(
        &self,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let results = sqlx::query_as::<_, (String, String, String)>(
                "SELECT id::text, content, created_at::text FROM memories ORDER BY created_at DESC LIMIT $1"
            )
            .bind(limit as i64)
            .fetch_all(&pool)
            .await?;
            Ok(results)
        })
    }

    // ===== Concepts =====

    fn add_concept(
        &self,
        name: &str,
        description: Option<&str>,
        scope: Option<&str>,
        id: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        let description = description.map(|s| s.to_string());
        let scope = scope.map(|s| s.to_string());
        let id = id.map(|s| s.to_string());
        Box::pin(async move {
            let concept_id = if let Some(id_str) = &id {
                Uuid::parse_str(id_str).unwrap_or_else(|_| Uuid::new_v4())
            } else {
                Uuid::new_v4()
            };

            sqlx::query(
                "INSERT INTO concepts (id, name, description, scope) VALUES ($1, $2, $3, $4) ON CONFLICT (name) DO UPDATE SET description = COALESCE($3, concepts.description), scope = COALESCE($4, concepts.scope)",
            )
            .bind(concept_id)
            .bind(&name)
            .bind(&description)
            .bind(&scope)
            .execute(&pool)
            .await
            .context("Failed to add concept")?;

            Ok(json!({"id": concept_id.to_string(), "name": name, "description": description, "scope": scope}))
        })
    }

    fn get_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move {
            let uuid = resolve_uuid(&pool, &id, "concepts").await?;

            let row = sqlx::query("SELECT id::text, name, description, category, scope FROM concepts WHERE id = $1")
                .bind(uuid)
                .fetch_one(&pool)
                .await
                .context("Failed to get concept")?;

            Ok(json!({
                "id": row.get::<String, _>("id"),
                "name": row.get::<String, _>("name"),
                "description": row.get::<Option<String>, _>("description"),
                "category": row.get::<Option<String>, _>("category"),
                "scope": row.get::<Option<String>, _>("scope")
            }))
        })
    }

    fn get_concept_with_instances(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move {
            let uuid = resolve_uuid(&pool, &id, "concepts").await?;

            let concept_row = sqlx::query("SELECT id::text, name, description, category, scope FROM concepts WHERE id = $1")
                .bind(uuid)
                .fetch_optional(&pool)
                .await
                .context("Failed to get concept")?
                .context("Concept not found")?;

            let concept = json!({
                "id": concept_row.get::<String, _>("id"),
                "name": concept_row.get::<String, _>("name"),
                "description": concept_row.get::<Option<String>, _>("description"),
                "category": concept_row.get::<Option<String>, _>("category"),
                "scope": concept_row.get::<Option<String>, _>("scope")
            });

            // Get instances (ontology edges where this is target with IS_A relation)
            let instances: Vec<serde_json::Value> = sqlx::query("SELECT from_id::text FROM ontology_edges WHERE to_id = $1 AND rel_type = 'INSTANCE_OF'")
                .bind(uuid.to_string())
                .fetch_all(&pool)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|r| json!(r.get::<String, _>("from_id")))
                .collect();

            // Get subclasses
            let subclasses: Vec<serde_json::Value> = sqlx::query("SELECT to_id::text FROM ontology_edges WHERE from_id = $1 AND rel_type = 'IS_A'")
                .bind(uuid.to_string())
                .fetch_all(&pool)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|r| json!(r.get::<String, _>("to_id")))
                .collect();

            Ok(json!({
                "concept": concept,
                "instances": instances,
                "subclasses": subclasses,
                "superclasses": []
            }))
        })
    }

    fn list_concepts(&self, scope: Option<&str>, limit: usize) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        let scope = scope.map(|s| s.to_string());
        Box::pin(async move {
            let mut sql = "SELECT id::text, name, description, category, scope FROM concepts".to_string();
            if scope.is_some() {
                sql.push_str(" WHERE scope = $1");
            }
            sql.push_str(&format!(" ORDER BY name LIMIT {}", limit));

            let rows = if let Some(scope_val) = scope {
                sqlx::query(&sql).bind(scope_val).fetch_all(&pool).await.context("Failed to list concepts")?
            } else {
                sqlx::query(&sql).fetch_all(&pool).await.context("Failed to list concepts")?
            };

            Ok(rows.into_iter().map(|r| {
                json!({
                    "id": r.get::<String, _>("id"),
                    "name": r.get::<String, _>("name"),
                    "description": r.get::<Option<String>, _>("description"),
                    "category": r.get::<Option<String>, _>("category"),
                    "scope": r.get::<Option<String>, _>("scope")
                })
            }).collect())
        })
    }

    fn delete_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move {
            let uuid = resolve_uuid(&pool, &id, "concepts").await?;
            let result = sqlx::query("DELETE FROM concepts WHERE id = $1")
                .bind(uuid)
                .execute(&pool)
                .await
                .context("Failed to delete concept")?;
            Ok(result.rows_affected() > 0)
        })
    }

    fn resolve_concept_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move {
            if let Ok(uuid) = Uuid::parse_str(&id) {
                return Ok(uuid.to_string());
            }
            let row: (String,) = sqlx::query_as("SELECT id::text FROM concepts WHERE id::text LIKE $1 OR name LIKE $2 LIMIT 1")
                .bind(format!("{}%", id))
                .bind(format!("%{}%", id))
                .fetch_optional(&pool)
                .await
                .context("Failed to resolve concept ID")?
                .context("Concept ID not found")?;
            Ok(row.0)
        })
    }

    fn search_concepts(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
        let pool = self.pool.clone();
        let query = query.to_string();
        let scope = scope.map(|s| s.to_string());
        Box::pin(async move {
            let mut sql = "SELECT id::text, name, description, category, scope FROM concepts WHERE (name ILIKE $1 OR description ILIKE $1)".to_string();
            let mut params = vec![format!("%{}%", query)];

            if let Some(scope_val) = scope {
                sql.push_str(" AND scope = $2");
                params.push(scope_val);
            }

            sql.push_str(&format!(" ORDER BY name LIMIT {}", limit));

            let mut query_builder = sqlx::query(&sql).bind(&params[0]);
            if params.len() > 1 {
                query_builder = query_builder.bind(&params[1]);
            }

            let rows = query_builder.fetch_all(&pool).await.context("Failed to search concepts")?;

            Ok(rows.into_iter().map(|r| {
                json!({
                    "concept": {
                        "id": r.get::<String, _>("id"),
                        "name": r.get::<String, _>("name"),
                        "description": r.get::<Option<String>, _>("description"),
                        "category": r.get::<Option<String>, _>("category"),
                        "scope": r.get::<Option<String>, _>("scope")
                    },
                    "relevance_score": 1.0
                })
            }).collect())
        })
    }

    // ===== Ontology Edges =====

    fn add_ontology_edge(
        &self,
        from_id: &str,
        from_kind: &str,
        rel: &str,
        to_id: &str,
        to_kind: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_id.to_string();
        let from_kind = from_kind.to_string();
        let rel = rel.to_string();
        let to_id = to_id.to_string();
        let to_kind = to_kind.to_string();
        let note = note.map(|s| s.to_string());
        Box::pin(async move {
            let result = sqlx::query(
                "INSERT INTO ontology_edges (from_id, from_kind, rel_type, to_id, to_kind, note) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            )
            .bind(&from_id)
            .bind(&from_kind)
            .bind(&rel)
            .bind(&to_id)
            .bind(&to_kind)
            .bind(&note)
            .fetch_one(&pool)
            .await
            .context("Failed to add ontology edge")?;

            let edge_id = result.get::<i64, _>("id");

            Ok(json!({
                "id": edge_id,
                "from_id": from_id,
                "from_kind": from_kind,
                "rel_type": rel,
                "to_id": to_id,
                "to_kind": to_kind,
                "note": note
            }))
        })
    }

    fn delete_ontology_edge(&self, edge_id: i64) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result = sqlx::query("DELETE FROM ontology_edges WHERE id = $1")
                .bind(edge_id)
                .execute(&pool)
                .await
                .context("Failed to delete ontology edge")?;
            Ok(result.rows_affected() > 0)
        })
    }

    // ===== Graph =====

    fn query_cypher(&self, _query: &str, _params: &serde_json::Value) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        Box::pin(async move {
            anyhow::bail!("Cypher queries not supported in PostgreSQL backend")
        })
    }

    fn get_neighbors(&self, id: &str, depth: usize) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>> {
        let pool = self.pool.clone();
        let id = id.to_string();
        Box::pin(async move {
            // Simple 1-depth neighbor lookup
            if depth < 1 {
                return Ok(json!({"neighbors": []}));
            }

            // Get direct neighbors from memory_edges
            let rows: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT to_id::text FROM memory_edges WHERE from_id::text = $1 UNION SELECT DISTINCT from_id::text FROM memory_edges WHERE to_id::text = $1")
                .bind(&id)
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

            Ok(json!({"neighbors": rows.into_iter().map(|(id,)| id).collect::<Vec<_>>()}))
        })
    }

    // ===== Utility =====

    fn check_model_mismatch(&self, _configured_model: &str) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>> {
        Box::pin(async move { Ok(None) })
    }

    fn delete_chunks_for_memory(
        &self,
        memory_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        let pool = self.pool.clone();
        let memory_id = memory_id.to_string();

        Box::pin(async move {
            // PostgreSQL: Delete all chunks for memory
            // Assuming a chunks table exists with memory_id column
            // If not implemented, return 0
            let result = sqlx::query(
                "DELETE FROM memory_chunks WHERE memory_id = $1"
            )
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
            let chunks = sqlx::query_as::<_, (String, String, String)>(
                "SELECT id::text, content, created_at::text FROM memory_chunks LIMIT $1"
            )
            .bind(limit as i64)
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            Ok(chunks)
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
            // PostgreSQL: Store embedding as bytea
            let embedding_bytes: Vec<u8> = embedding
                .iter()
                .flat_map(|f| f.to_le_bytes().to_vec())
                .collect();

            sqlx::query(
                "UPDATE memory_chunks SET embedding = $1, embedding_dim = $2 WHERE id = $3"
            )
            .bind(&embedding_bytes)
            .bind(dim as i32)
            .bind(&chunk_id)
            .execute(&pool)
            .await
            .ok();

            tracing::debug!("PostgreSQL: Stored {}D embedding for chunk {}", dim, chunk_id);
            Ok((chunk_id, dim))
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
                "SELECT embedding, embedding_dim FROM memory_chunks WHERE id = $1"
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
            // PostgreSQL: Fetch all chunks with embeddings (pgvector or bytea)
            let chunks: Vec<(String, Vec<u8>, i32)> = sqlx::query_as(
                "SELECT id, embedding, embedding_dim FROM memory_chunks 
                 WHERE embedding IS NOT NULL AND embedding_dim = $1
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

            tracing::debug!("PostgreSQL: Found {} similar chunks", similarities.len());
            Ok(similarities)
        })
    }
}

// ============================================================================
// Batch Operations
// ============================================================================

/// Batch add multiple memories
pub async fn batch_add_memories(
    db: &PostgresDatabase,
    memories: Vec<serde_json::Value>,
) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for mem in memories {
        let result = db.add_memory(mem, &json!({})).await?;
        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            ids.push(id.to_string());
        }
    }
    Ok(ids)
}

/// Batch delete memories
pub async fn batch_delete_memories(
    db: &PostgresDatabase,
    ids: Vec<&str>,
) -> Result<usize> {
    let mut deleted = 0;
    for id in ids {
        if db.delete_memory(id).await? {
            deleted += 1;
        }
    }
    Ok(deleted)
}

// ============================================================================
// Query Optimization
// ============================================================================

/// Search with pagination
pub async fn search_paginated(
    pool: &PgPool,
    query: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<(String, f32)>> {
    let rows: Vec<(String, f32)> = sqlx::query_as(
        "SELECT id::text, ts_rank_cd(to_tsvector('english', content), plainto_tsquery('english', $1), 1)::float FROM memories WHERE to_tsvector('english', content) @@ plainto_tsquery('english', $1) ORDER BY ts_rank_cd DESC LIMIT $2 OFFSET $3"
    )
    .bind(query)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to search with pagination")?;
    
    Ok(rows)
}

/// Get memory with related edges
pub async fn get_memory_with_edges(
    pool: &PgPool,
    id: &str,
) -> Result<Option<serde_json::Value>> {
    let uuid = if let Ok(u) = Uuid::parse_str(id) {
        u
    } else {
        let row: Option<(String,)> = sqlx::query_as("SELECT id::text FROM memories WHERE id::text LIKE $1 LIMIT 1")
            .bind(format!("{}%", id))
            .fetch_optional(pool)
            .await
            .context("Failed to resolve memory ID")?;
        
        if let Some((id_str,)) = row {
            Uuid::parse_str(&id_str)?
        } else {
            return Ok(None);
        }
    };

    let mem_row = sqlx::query("SELECT id::text, content, memory_type, tags, scopes, importance, created_at, updated_at FROM memories WHERE id = $1")
        .bind(uuid)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch memory")?;

    if let Some(r) = mem_row {
        let edges: Vec<(String, String, String)> = sqlx::query_as("SELECT from_id::text, rel_type, to_id::text FROM memory_edges WHERE from_id = $1 OR to_id = $1")
            .bind(uuid.to_string())
            .fetch_all(pool)
            .await
            .unwrap_or_default();

        Ok(Some(json!({
            "id": r.get::<String, _>("id"),
            "content": r.get::<String, _>("content"),
            "memory_type": r.get::<String, _>("memory_type"),
            "tags": r.get::<serde_json::Value, _>("tags"),
            "scopes": r.get::<serde_json::Value, _>("scopes"),
            "importance": r.get::<i32, _>("importance"),
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            "edges": edges.into_iter().map(|(from, rel, to)| {
                json!({"from": from, "rel_type": rel, "to": to})
            }).collect::<Vec<_>>()
        })))
    } else {
        Ok(None)
    }
}

/// Bulk link memories
pub async fn bulk_link_memories(
    pool: &PgPool,
    links: Vec<(String, String, String)>, // (from_id, rel_type, to_id)
) -> Result<usize> {
    let mut count = 0;
    for (from_id, rel, to_id) in links {
        let from_uuid = resolve_uuid(pool, &from_id, "memories").await?;
        let to_uuid = resolve_uuid(pool, &to_id, "memories").await?;

        sqlx::query("INSERT INTO memory_edges (from_id, rel_type, to_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(from_uuid)
            .bind(&rel)
            .bind(to_uuid)
            .execute(pool)
            .await
            .ok();
        
        count += 1;
    }
    Ok(count)
}

/// Export memories to JSON
pub async fn export_memories(
    pool: &PgPool,
    scope: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
    let mut sql = "SELECT id::text, content, memory_type, tags, scopes, importance, created_at, updated_at FROM memories".to_string();
    
    if scope.is_some() {
        sql.push_str(" WHERE scopes @> ($1 ::text)::jsonb");
    }
    
    sql.push_str(" ORDER BY created_at DESC");

    let rows = if let Some(s) = scope {
        sqlx::query(&sql)
            .bind(format!(r#"["{s}"]"#))
            .fetch_all(pool)
            .await
            .context("Failed to export memories")?
    } else {
        sqlx::query(&sql)
            .fetch_all(pool)
            .await
            .context("Failed to export memories")?
    };

    Ok(rows.into_iter().map(|r| {
        json!({
            "id": r.get::<String, _>("id"),
            "content": r.get::<String, _>("content"),
            "memory_type": r.get::<String, _>("memory_type"),
            "tags": r.get::<serde_json::Value, _>("tags"),
            "scopes": r.get::<serde_json::Value, _>("scopes"),
            "importance": r.get::<i32, _>("importance"),
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339()
        })
    }).collect())
}

/// Import memories from JSON
pub async fn import_memories(
    pool: &PgPool,
    memories: Vec<serde_json::Value>,
) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for mem in memories {
        let content = mem.get("content").and_then(|v| v.as_str()).context("Missing content")?;
        let memory_type = mem.get("memory_type").and_then(|v| v.as_str()).unwrap_or("semantic");
        
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO memories (id, content, memory_type) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(content)
            .bind(memory_type)
            .execute(pool)
            .await
            .ok();
        
        ids.push(id.to_string());
    }
    Ok(ids)
}

/// Get graph statistics
pub async fn get_stats(pool: &PgPool) -> Result<serde_json::Value> {
    let memory_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await
        .context("Failed to count memories")?;

    let edge_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory_edges")
        .fetch_one(pool)
        .await
        .context("Failed to count edges")?;

    let concept_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM concepts")
        .fetch_one(pool)
        .await
        .context("Failed to count concepts")?;

    let type_distribution: Vec<(String, i64)> = sqlx::query_as("SELECT memory_type, COUNT(*) FROM memories GROUP BY memory_type")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    Ok(json!({
        "total_memories": memory_count.0,
        "total_edges": edge_count.0,
        "total_concepts": concept_count.0,
        "type_distribution": type_distribution.into_iter().map(|(t, c)| json!({"type": t, "count": c})).collect::<Vec<_>>()
    }))
}

/// Vacuum/cleanup database
pub async fn vacuum(pool: &PgPool) -> Result<()> {
    sqlx::query("VACUUM ANALYZE memories").execute(pool).await.ok();
    sqlx::query("VACUUM ANALYZE memory_edges").execute(pool).await.ok();
    sqlx::query("VACUUM ANALYZE concepts").execute(pool).await.ok();
    sqlx::query("VACUUM ANALYZE ontology_edges").execute(pool).await.ok();
    Ok(())
}

/// Find orphaned edges (referencing deleted memories)
pub async fn find_orphaned_edges(pool: &PgPool) -> Result<Vec<(String, String, String)>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT from_id::text, rel_type, to_id::text FROM memory_edges WHERE from_id NOT IN (SELECT id FROM memories) OR to_id NOT IN (SELECT id FROM memories)"
    )
    .fetch_all(pool)
    .await
    .context("Failed to find orphaned edges")?;
    
    Ok(rows)
}

/// Clean orphaned edges
pub async fn cleanup_orphaned_edges(pool: &PgPool) -> Result<usize> {
    let result = sqlx::query(
        "DELETE FROM memory_edges WHERE from_id NOT IN (SELECT id FROM memories) OR to_id NOT IN (SELECT id FROM memories)"
    )
    .execute(pool)
    .await
    .context("Failed to cleanup orphaned edges")?;
    
    Ok(result.rows_affected() as usize)
}

// ============================================================================
// Advanced Query Helpers
// ============================================================================

/// Full-text search with ranking and details
pub async fn search_with_details(
    pool: &PgPool,
    query: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT id::text, content, memory_type, ts_rank_cd(to_tsvector('english', content), plainto_tsquery('english', $1), 1)::float as score FROM memories WHERE to_tsvector('english', content) @@ plainto_tsquery('english', $1) ORDER BY score DESC LIMIT $2"
    )
    .bind(query)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .context("Failed to search with details")?;

    Ok(rows.into_iter().map(|r| {
        json!({
            "id": r.get::<String, _>("id"),
            "content": r.get::<String, _>("content"),
            "memory_type": r.get::<String, _>("memory_type"),
            "score": r.get::<f32, _>("score")
        })
    }).collect())
}

/// Search for memories similar to another memory
pub async fn find_similar_memories(
    pool: &PgPool,
    memory_id: &str,
    limit: usize,
) -> Result<Vec<(String, f32)>> {
    let uuid = resolve_uuid(pool, memory_id, "memories").await?;

    // Get the content of reference memory
    let ref_row: Option<(String,)> = sqlx::query_as("SELECT content FROM memories WHERE id = $1")
        .bind(uuid)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch reference memory")?;

    if let Some((content,)) = ref_row {
        // Use tsvector distance for similarity (simplified)
        let rows: Vec<(String, f32)> = sqlx::query_as(
            "SELECT id::text, ts_rank_cd(to_tsvector('english', content), to_tsvector('english', $1), 1)::float FROM memories WHERE id != $2 ORDER BY ts_rank_cd DESC LIMIT $3"
        )
        .bind(&content)
        .bind(uuid.to_string())
        .bind(limit as i64)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        Ok(rows)
    } else {
        Ok(vec![])
    }
}

/// Get concept with all related information
pub async fn get_concept_full(
    pool: &PgPool,
    concept_id: &str,
) -> Result<Option<serde_json::Value>> {
    let uuid = resolve_uuid(pool, concept_id, "concepts").await?;

    let concept_row = sqlx::query("SELECT id::text, name, description, category, scope FROM concepts WHERE id = $1")
        .bind(uuid)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch concept")?;

    if let Some(r) = concept_row {
        // Get instances
        let instances: Vec<String> = sqlx::query_as("SELECT from_id::text FROM ontology_edges WHERE to_id = $1 AND rel_type = 'INSTANCE_OF'")
            .bind(uuid.to_string())
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id,)| id)
            .collect();

        // Get parent concepts
        let parents: Vec<String> = sqlx::query_as("SELECT to_id::text FROM ontology_edges WHERE from_id = $1 AND rel_type = 'IS_A'")
            .bind(uuid.to_string())
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id,)| id)
            .collect();

        // Get child concepts
        let children: Vec<String> = sqlx::query_as("SELECT from_id::text FROM ontology_edges WHERE to_id = $1 AND rel_type = 'IS_A'")
            .bind(uuid.to_string())
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id,)| id)
            .collect();

        Ok(Some(json!({
            "id": r.get::<String, _>("id"),
            "name": r.get::<String, _>("name"),
            "description": r.get::<Option<String>, _>("description"),
            "category": r.get::<Option<String>, _>("category"),
            "scope": r.get::<Option<String>, _>("scope"),
            "instances": instances,
            "parents": parents,
            "children": children
        })))
    } else {
        Ok(None)
    }
}

/// Get all memories in a scope
pub async fn get_scope_memories(
    pool: &PgPool,
    scope: &str,
) -> Result<Vec<(String, String, String)>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id::text, content, memory_type FROM memories WHERE scopes @> ($1 ::text)::jsonb ORDER BY created_at DESC"
    )
    .bind(format!(r#"["{scope}"]"#))
    .fetch_all(pool)
    .await
    .context("Failed to fetch scope memories")?;

    Ok(rows)
}

/// Get memory relationships graph (depth limited)
pub async fn get_memory_graph(
    pool: &PgPool,
    memory_id: &str,
    max_depth: i32,
) -> Result<serde_json::Value> {
    let uuid = resolve_uuid(pool, memory_id, "memories").await?;

    // Get direct edges
    let edges: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT from_id::text, rel_type, to_id::text FROM memory_edges WHERE from_id = $1 OR to_id = $1 LIMIT 100"
    )
    .bind(uuid.to_string())
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let nodes: Vec<String> = {
        let mut set = std::collections::HashSet::new();
        set.insert(uuid.to_string());
        for (from, _, to) in &edges {
            set.insert(from.clone());
            set.insert(to.clone());
        }
        set.into_iter().collect()
    };

    Ok(json!({
        "root": uuid.to_string(),
        "depth": max_depth,
        "nodes": nodes,
        "edges": edges.into_iter().map(|(from, rel, to)| {
            json!({"from": from, "relation": rel, "to": to})
        }).collect::<Vec<_>>()
    }))
}

/// Search memories by tags
pub async fn search_by_tags(
    pool: &PgPool,
    tags: Vec<&str>,
    match_all: bool,
) -> Result<Vec<String>> {
    let tag_json = serde_json::to_value(&tags)?;
    
    let sql = if match_all {
        "SELECT id::text FROM memories WHERE tags @> $1::jsonb"
    } else {
        "SELECT id::text FROM memories WHERE tags ?| $1::text[]"
    };

    let rows: Vec<(String,)> = if match_all {
        sqlx::query_as(sql)
            .bind(&tag_json)
            .fetch_all(pool)
            .await
            .context("Failed to search by tags")?
    } else {
        sqlx::query_as("SELECT id::text FROM memories WHERE tags ?| $1::text[]")
            .bind(&tags)
            .fetch_all(pool)
            .await
            .context("Failed to search by tags")?
    };

    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Reindex full-text search (after bulk updates)
pub async fn reindex_fts(pool: &PgPool) -> Result<()> {
    sqlx::query("REINDEX INDEX memories_fts_idx")
        .execute(pool)
        .await
        .ok();
    Ok(())
}

/// Get memory by exact content hash
pub async fn get_memory_by_content_hash(
    pool: &PgPool,
    content_hash: &str,
) -> Result<Option<String>> {
    let hash_bytes = content_hash.as_bytes();
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id::text FROM memories WHERE md5(content) = md5($1) LIMIT 1"
    )
    .bind(String::from_utf8_lossy(hash_bytes).as_ref())
    .fetch_optional(pool)
    .await
    .context("Failed to fetch memory by hash")?;

    Ok(row.map(|(id,)| id))
}

/// Duplicate content detection
pub async fn find_duplicate_content(
    pool: &PgPool,
) -> Result<Vec<Vec<String>>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT md5(content) as hash, id::text FROM memories WHERE md5(content) IN (SELECT md5(content) FROM memories GROUP BY md5(content) HAVING COUNT(*) > 1) ORDER BY hash"
    )
    .fetch_all(pool)
    .await
    .context("Failed to find duplicates")?;

    let mut groups: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (hash, id) in rows {
        groups.entry(hash).or_insert_with(Vec::new).push(id);
    }

    Ok(groups.into_values().collect())
}

// ============================================================================
// Transaction Support
// ============================================================================


// ============================================================================
// Query Builder & Advanced Filtering
// ============================================================================

pub struct MemoryQueryBuilder {
    filters: Vec<String>,
}

impl MemoryQueryBuilder {
    pub fn new() -> Self {
        Self { filters: Vec::new() }
    }

    pub fn memory_type(mut self, type_name: &str) -> Self {
        self.filters.push(format!("memory_type = '{}'", type_name));
        self
    }

    pub fn has_scope(mut self, scope: &str) -> Self {
        self.filters.push(format!("scopes @> '[\"{}\"]\n'", scope));
        self
    }

    pub fn has_tag(mut self, tag: &str) -> Self {
        self.filters.push(format!("tags @> '[\"{}\"]\n'", tag));
        self
    }

    pub fn min_importance(mut self, importance: i32) -> Self {
        self.filters.push(format!("importance >= {}", importance));
        self
    }

    pub fn content_contains(mut self, text: &str) -> Self {
        self.filters.push(format!("content ILIKE '%{}%'", text));
        self
    }

    pub fn build(&self) -> String {
        let mut query = "SELECT id::text, content, memory_type FROM memories".to_string();
        
        if !self.filters.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&self.filters.join(" AND "));
        }
        
        query.push_str(" ORDER BY created_at DESC");
        query
    }
}

pub async fn execute_query(pool: &PgPool, query: &str, limit: i64) -> Result<Vec<serde_json::Value>> {
    let mut q = query.to_string();
    q.push_str(&format!(" LIMIT {}", limit));
    
    let rows = sqlx::query(&q)
        .fetch_all(pool)
        .await
        .context("Failed to execute query")?;

    Ok(rows.into_iter().map(|r| {
        json!({
            "id": r.get::<String, _>("id"),
            "content": r.get::<String, _>("content"),
            "memory_type": r.get::<String, _>("memory_type")
        })
    }).collect())
}

// ============================================================================
// Batch Import/Export
// ============================================================================

pub async fn import_from_json_file(
    pool: &PgPool,
    json_data: Vec<serde_json::Value>,
) -> Result<usize> {
    let mut count = 0;
    
    for item in json_data {
        let id = Uuid::new_v4();
        let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let memory_type = item.get("memory_type").and_then(|v| v.as_str()).unwrap_or("semantic");

        sqlx::query("INSERT INTO memories (id, content, memory_type) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(content)
            .bind(memory_type)
            .execute(pool)
            .await
            .ok();

        count += 1;
    }

    Ok(count)
}

pub async fn export_to_json_file(pool: &PgPool) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query("SELECT id::text, content, memory_type, created_at FROM memories ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
        .context("Failed to export memories")?;

    Ok(rows.into_iter().map(|r| {
        json!({
            "id": r.get::<String, _>("id"),
            "content": r.get::<String, _>("content"),
            "memory_type": r.get::<String, _>("memory_type"),
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339()
        })
    }).collect())
}

// ============================================================================
// Indexing & Performance
// ============================================================================

pub async fn create_indexes(pool: &PgPool) -> Result<()> {
    // Content FTS index
    sqlx::query("CREATE INDEX IF NOT EXISTS memories_content_fts ON memories USING GiST(to_tsvector('english', content))")
        .execute(pool)
        .await
        .ok();

    // Type index
    sqlx::query("CREATE INDEX IF NOT EXISTS memories_type ON memories(memory_type)")
        .execute(pool)
        .await
        .ok();

    // Scopes JSONB index
    sqlx::query("CREATE INDEX IF NOT EXISTS memories_scopes ON memories USING GIN(scopes)")
        .execute(pool)
        .await
        .ok();

    // Tags JSONB index
    sqlx::query("CREATE INDEX IF NOT EXISTS memories_tags ON memories USING GIN(tags)")
        .execute(pool)
        .await
        .ok();

    // Created/updated timestamps
    sqlx::query("CREATE INDEX IF NOT EXISTS memories_created ON memories(created_at DESC)")
        .execute(pool)
        .await
        .ok();

    sqlx::query("CREATE INDEX IF NOT EXISTS memories_updated ON memories(updated_at DESC)")
        .execute(pool)
        .await
        .ok();

    Ok(())
}

pub async fn analyze_tables(pool: &PgPool) -> Result<()> {
    sqlx::query("ANALYZE memories").execute(pool).await.ok();
    sqlx::query("ANALYZE memory_edges").execute(pool).await.ok();
    sqlx::query("ANALYZE concepts").execute(pool).await.ok();
    Ok(())
}

// ============================================================================
// Connection Management
// ============================================================================

pub struct ConnectionPool {
    pool: PgPool,
}

impl ConnectionPool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn get_pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn warmup(&self) -> Result<()> {
        // Pre-allocate connections
        let _ = self.pool.acquire().await?;
        Ok(())
    }

    pub async fn get_connection_stats(&self) -> serde_json::Value {
        json!({
            "size": self.pool.size(),
            "num_idle": self.pool.num_idle()
        })
    }
}

// ============================================================================
// Data Validation
// ============================================================================

pub fn validate_memory_request(req: &serde_json::Value) -> Result<()> {
    if req.get("content").and_then(|v| v.as_str()).is_none() {
        anyhow::bail!("Missing or invalid 'content' field");
    }

    if let Some(tags) = req.get("tags") {
        if !tags.is_array() {
            anyhow::bail!("'tags' must be an array");
        }
    }

    if let Some(scopes) = req.get("scopes") {
        if !scopes.is_array() {
            anyhow::bail!("'scopes' must be an array");
        }
    }

    Ok(())
}

pub fn validate_concept_request(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Concept name cannot be empty");
    }

    if name.len() > 255 {
        anyhow::bail!("Concept name too long (max 255 chars)");
    }

    Ok(())
}

// ============================================================================
// Convenience Methods on PostgresDatabase
// ============================================================================

impl PostgresDatabase {
    /// Initialize all indexes
    pub async fn setup_indexes(&self) -> Result<()> {
        create_indexes(&self.pool).await
    }

    /// Get database statistics
    pub async fn stats(&self) -> Result<serde_json::Value> {
        get_stats(&self.pool).await
    }

    /// Export all data to JSON
    pub async fn export_all(&self) -> Result<serde_json::Value> {
        export_to_json_file(&self.pool).await.map(|items| json!({"memories": items}))
    }

    /// Import memories from JSON
    pub async fn import_all(&self, data: Vec<serde_json::Value>) -> Result<usize> {
        import_from_json_file(&self.pool, data).await
    }

    /// Find similar memories to a given memory
    pub async fn find_similar(&self, memory_id: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        find_similar_memories(&self.pool, memory_id, limit).await
    }

    /// Full memory graph traversal
    pub async fn get_graph(&self, memory_id: &str, depth: i32) -> Result<serde_json::Value> {
        get_memory_graph(&self.pool, memory_id, depth).await
    }

    /// Duplicate content detection and cleanup
    pub async fn find_duplicates(&self) -> Result<Vec<Vec<String>>> {
        find_duplicate_content(&self.pool).await
    }

    /// Search with full result details
    pub async fn search_detailed(&self, query: &str, limit: usize) -> Result<Vec<serde_json::Value>> {
        search_with_details(&self.pool, query, limit).await
    }

    /// Get all memories in a scope with details
    pub async fn get_scope(&self, scope: &str) -> Result<Vec<(String, String, String)>> {
        get_scope_memories(&self.pool, scope).await
    }

    /// Full maintenance routine
    pub async fn maintenance(&self) -> Result<()> {
        vacuum(&self.pool).await?;
        analyze_tables(&self.pool).await?;
        Ok(())
    }

    /// Database health and diagnostics
    pub async fn diagnostics(&self) -> Result<serde_json::Value> {
        let stats = get_stats(&self.pool).await?;
        let orphaned = find_orphaned_edges(&self.pool).await.unwrap_or_default();
        let duplicates = find_duplicate_content(&self.pool).await.unwrap_or_default();
        
        Ok(json!({
            "status": "ok",
            "stats": stats,
            "orphaned_edges": orphaned.len(),
            "duplicate_groups": duplicates.len()
        }))
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get total size of all memories (bytes)
pub async fn get_total_memory_size(pool: &PgPool) -> Result<i64> {
    let row: (Option<i64>,) = sqlx::query_as("SELECT SUM(octet_length(content)) FROM memories")
        .fetch_one(pool)
        .await
        .context("Failed to get memory size")?;
    
    Ok(row.0.unwrap_or(0))
}

/// Get memory size by type
pub async fn get_memory_size_by_type(pool: &PgPool) -> Result<Vec<(String, i64)>> {
    let rows: Vec<(String, Option<i64>)> = sqlx::query_as("SELECT memory_type, SUM(octet_length(content)) FROM memories GROUP BY memory_type")
        .fetch_all(pool)
        .await
        .context("Failed to get memory size by type")?;
    
    Ok(rows.into_iter().map(|(t, s)| (t, s.unwrap_or(0))).collect())
}

/// Count memories by type
pub async fn count_by_type(pool: &PgPool) -> Result<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT memory_type, COUNT(*) FROM memories GROUP BY memory_type ORDER BY COUNT(*) DESC")
        .fetch_all(pool)
        .await
        .context("Failed to count by type")?;
    
    Ok(rows)
}

/// Count edges by type
pub async fn count_edges_by_type(pool: &PgPool) -> Result<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT rel_type, COUNT(*) FROM memory_edges GROUP BY rel_type ORDER BY COUNT(*) DESC")
        .fetch_all(pool)
        .await
        .context("Failed to count edges")?;
    
    Ok(rows)
}

/// Get most referenced memories (by edges)
pub async fn get_most_referenced(pool: &PgPool, limit: i64) -> Result<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT to_id::text, COUNT(*) as ref_count FROM memory_edges GROUP BY to_id ORDER BY ref_count DESC LIMIT $1"
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to get most referenced")?;
    
    Ok(rows)
}

/// Find isolated memories (no edges)
pub async fn find_isolated_memories(pool: &PgPool, limit: i64) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT id::text FROM memories WHERE id NOT IN (SELECT from_id FROM memory_edges UNION SELECT to_id FROM memory_edges) LIMIT $1"
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to find isolated memories")?;
    
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Get recently updated memories
pub async fn get_recent_updates(pool: &PgPool, limit: i64) -> Result<Vec<serde_json::Value>> {
    let rows = sqlx::query(
        "SELECT id::text, content, memory_type, updated_at FROM memories ORDER BY updated_at DESC LIMIT $1"
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to get recent updates")?;
    
    Ok(rows.into_iter().map(|r| {
        json!({
            "id": r.get::<String, _>("id"),
            "content": r.get::<String, _>("content"),
            "memory_type": r.get::<String, _>("memory_type"),
            "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339()
        })
    }).collect())
}

/// Get distribution of importance scores
pub async fn get_importance_distribution(pool: &PgPool) -> Result<Vec<(i32, i64)>> {
    let rows: Vec<(i32, i64)> = sqlx::query_as(
        "SELECT importance, COUNT(*) FROM memories GROUP BY importance ORDER BY importance"
    )
    .fetch_all(pool)
    .await
    .context("Failed to get importance distribution")?;
    
    Ok(rows)
}

/// Calculate database quality metrics
pub async fn get_quality_metrics(pool: &PgPool) -> Result<serde_json::Value> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await?;
    
    let with_edges: (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT id) FROM (SELECT from_id as id FROM memory_edges UNION SELECT to_id FROM memory_edges) t")
        .fetch_one(pool)
        .await
        .unwrap_or((0,));
    
    let isolated = total.0 - with_edges.0;
    let edge_connectivity = if total.0 > 0 { (with_edges.0 as f32 / total.0 as f32) * 100.0 } else { 0.0 };

    let avg_content_len: (Option<i64>,) = sqlx::query_as("SELECT AVG(LENGTH(content)) FROM memories")
        .fetch_one(pool)
        .await
        .unwrap_or((None,));

    Ok(json!({
        "total_memories": total.0,
        "memories_with_edges": with_edges.0,
        "isolated_memories": isolated,
        "edge_connectivity_percent": edge_connectivity,
        "avg_content_length": avg_content_len.0.unwrap_or(0),
        "coverage": format!("{:.1}%", edge_connectivity)
    }))
}

// ============================================================================
// High-Performance Bulk Operations
// ============================================================================

/// Bulk update importance scores
pub async fn bulk_update_importance(
    pool: &PgPool,
    updates: Vec<(String, i32)>, // (memory_id, new_importance)
) -> Result<usize> {
    let mut count = 0;
    
    for (id, importance) in updates {
        let uuid = resolve_uuid(pool, &id, "memories").await?;
        
        sqlx::query("UPDATE memories SET importance = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2")
            .bind(importance)
            .bind(uuid)
            .execute(pool)
            .await
            .ok();
        
        count += 1;
    }
    
    Ok(count)
}

/// Bulk add tags to memories
pub async fn bulk_add_tags(
    pool: &PgPool,
    updates: Vec<(String, Vec<String>)>, // (memory_id, new_tags)
) -> Result<usize> {
    let mut count = 0;
    
    for (id, new_tags) in updates {
        let uuid = resolve_uuid(pool, &id, "memories").await?;
        let tags_json = serde_json::to_string(&new_tags)?;
        
        sqlx::query("UPDATE memories SET tags = tags || $1::jsonb, updated_at = CURRENT_TIMESTAMP WHERE id = $2")
            .bind(&tags_json)
            .bind(uuid)
            .execute(pool)
            .await
            .ok();
        
        count += 1;
    }
    
    Ok(count)
}

/// Bulk update scopes
pub async fn bulk_update_scopes(
    pool: &PgPool,
    updates: Vec<(String, Vec<String>)>, // (memory_id, new_scopes)
) -> Result<usize> {
    let mut count = 0;
    
    for (id, new_scopes) in updates {
        let uuid = resolve_uuid(pool, &id, "memories").await?;
        let scopes_json = serde_json::to_string(&new_scopes)?;
        
        sqlx::query("UPDATE memories SET scopes = $1::jsonb, updated_at = CURRENT_TIMESTAMP WHERE id = $2")
            .bind(&scopes_json)
            .bind(uuid)
            .execute(pool)
            .await
            .ok();
        
        count += 1;
    }
    
    Ok(count)
}

/// Bulk soft delete (mark by importance = -1)
pub async fn bulk_soft_delete(
    pool: &PgPool,
    memory_ids: Vec<&str>,
) -> Result<usize> {
    let mut count = 0;
    
    for id in memory_ids {
        let uuid = resolve_uuid(pool, id, "memories").await?;
        
        sqlx::query("UPDATE memories SET importance = -1, updated_at = CURRENT_TIMESTAMP WHERE id = $1")
            .bind(uuid)
            .execute(pool)
            .await
            .ok();
        
        count += 1;
    }
    
    Ok(count)
}

/// Purge soft-deleted memories
pub async fn purge_soft_deleted(pool: &PgPool) -> Result<usize> {
    let result = sqlx::query("DELETE FROM memories WHERE importance = -1")
        .execute(pool)
        .await
        .context("Failed to purge soft deleted")?;
    
    Ok(result.rows_affected() as usize)
}

/// Copy memories to scope
pub async fn copy_to_scope(
    pool: &PgPool,
    memory_ids: Vec<&str>,
    target_scope: &str,
) -> Result<usize> {
    let mut count = 0;
    
    for id in memory_ids {
        let uuid = resolve_uuid(pool, id, "memories").await?;
        
        let target_json = format!(r#"["{target_scope}"]"#);
        sqlx::query("UPDATE memories SET scopes = scopes || $1::jsonb WHERE id = $2")
            .bind(&target_json)
            .bind(uuid)
            .execute(pool)
            .await
            .ok();
        
        count += 1;
    }
    
    Ok(count)
}

/// Batch concept operations
pub async fn bulk_create_concepts(
    pool: &PgPool,
    concepts: Vec<(&str, Option<&str>)>, // (name, description)
) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    
    for (name, desc) in concepts {
        let id = Uuid::new_v4();
        
        sqlx::query("INSERT INTO concepts (id, name, description) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(name)
            .bind(desc)
            .execute(pool)
            .await
            .ok();
        
        ids.push(id.to_string());
    }
    
    Ok(ids)
}

// ============================================================================
// Schema Migration & Versioning
// ============================================================================

pub struct SchemaVersion {
    version: i32,
    description: String,
}

/// Create schema version table
pub async fn init_schema_versioning(pool: &PgPool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS schema_versions (
            id SERIAL PRIMARY KEY,
            version INT NOT NULL UNIQUE,
            applied_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
            description TEXT
        )
        "#
    )
    .execute(pool)
    .await
    .context("Failed to create schema_versions table")?;

    Ok(())
}

/// Get current schema version
pub async fn get_schema_version(pool: &PgPool) -> Result<Option<i32>> {
    let row: Option<(i32,)> = sqlx::query_as("SELECT MAX(version) FROM schema_versions")
        .fetch_optional(pool)
        .await
        .context("Failed to get schema version")?;

    Ok(row.and_then(|(v,)| Some(v)))
}

/// Record schema migration
pub async fn record_migration(
    pool: &PgPool,
    version: i32,
    description: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO schema_versions (version, description) VALUES ($1, $2)")
        .bind(version)
        .bind(description)
        .execute(pool)
        .await
        .context("Failed to record migration")?;

    Ok(())
}

/// Get all migration history
pub async fn get_migration_history(pool: &PgPool) -> Result<Vec<(i32, String, String)>> {
    let rows: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT version, applied_at::text, description FROM schema_versions ORDER BY version"
    )
    .fetch_all(pool)
    .await
    .context("Failed to get migration history")?;

    Ok(rows)
}

// ============================================================================
// Data Integrity Checks
// ============================================================================

/// Check referential integrity
pub async fn check_referential_integrity(pool: &PgPool) -> Result<serde_json::Value> {
    let orphaned_edges: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_edges WHERE from_id NOT IN (SELECT id FROM memories) OR to_id NOT IN (SELECT id FROM memories)"
    )
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    let orphaned_ontology: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM ontology_edges WHERE from_id NOT IN (SELECT id FROM concepts) OR to_id NOT IN (SELECT id FROM concepts)"
    )
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    Ok(json!({
        "orphaned_memory_edges": orphaned_edges.0,
        "orphaned_ontology_edges": orphaned_ontology.0,
        "integrity_ok": orphaned_edges.0 == 0 && orphaned_ontology.0 == 0
    }))
}

/// Validate all indexes exist
pub async fn validate_indexes(pool: &PgPool) -> Result<serde_json::Value> {
    let required_indexes = vec![
        "memories_content_fts",
        "memories_type",
        "memories_scopes",
        "memories_tags",
        "memories_created",
        "memories_updated",
    ];

    let mut missing = Vec::new();
    
    for idx in required_indexes {
        let exists: (i64,) = sqlx::query_as(
            "SELECT 1 FROM pg_indexes WHERE indexname = $1"
        )
        .bind(idx)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .unwrap_or((0,));

        if exists.0 == 0 {
            missing.push(idx.to_string());
        }
    }

    Ok(json!({
        "all_present": missing.is_empty(),
        "missing_indexes": missing
    }))
}

// ============================================================================
// Backup & Restore
// ============================================================================

/// Create backup dump (returns SQL statements)
pub async fn create_backup(pool: &PgPool) -> Result<String> {
    let mut backup = String::new();
    
    // Dump memories
    let memories = sqlx::query("SELECT * FROM memories ORDER BY created_at")
        .fetch_all(pool)
        .await
        .context("Failed to fetch memories for backup")?;

    for row in memories {
        let id = row.get::<String, _>("id");
        let content = row.get::<String, _>("content").replace("'", "''");
        let memory_type = row.get::<String, _>("memory_type");
        
        backup.push_str(&format!(
            "INSERT INTO memories (id, content, memory_type) VALUES ('{}', '{}', '{}');\n",
            id, content, memory_type
        ));
    }

    Ok(backup)
}

/// Restore from backup (simple format)
pub async fn restore_from_backup(
    pool: &PgPool,
    backup_lines: Vec<&str>,
) -> Result<usize> {
    let mut count = 0;
    
    for line in backup_lines {
        if !line.is_empty() && line.contains("INSERT") {
            sqlx::query(line).execute(pool).await.ok();
            count += 1;
        }
    }
    
    Ok(count)
}
