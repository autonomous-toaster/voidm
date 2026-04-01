//! GraphQueryOps implementation for SQLite backend
//!
//! Implements all low-level graph query operations using sqlx.
//! This isolates database operations from domain logic (voidm-graph).

use anyhow::Result;
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::HashMap;
use voidm_db::graph_ops::GraphQueryOps;

/// SQLite implementation of GraphQueryOps
pub struct SqliteGraphQueryOps {
    pool: SqlitePool,
}

impl SqliteGraphQueryOps {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl GraphQueryOps for SqliteGraphQueryOps {
    // ===== Node Operations =====

    fn upsert_node(&self, memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i64>> + Send + '_>> {
        let pool = self.pool.clone();
        let memory_id = memory_id.to_string();
        Box::pin(async move {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Memory', '{}', ?, ?)")
                .bind(&memory_id)
                .bind(&now)
                .bind(&now)
                .execute(&pool)
                .await?;
            Ok(0)
        })
    }

    fn delete_node(&self, memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        let memory_id = memory_id.to_string();
        Box::pin(async move {
            sqlx::query("DELETE FROM nodes WHERE id = ?")
                .bind(&memory_id)
                .execute(&pool)
                .await?;
            Ok(())
        })
    }

    fn get_node_id(&self, memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<i64>>> + Send + '_>> {
        let pool = self.pool.clone();
        let memory_id = memory_id.to_string();
        Box::pin(async move {
            let exists: Option<String> = sqlx::query_scalar("SELECT id FROM nodes WHERE id = ?")
                .bind(&memory_id)
                .fetch_optional(&pool)
                .await?;
            Ok(exists.map(|_| 0))
        })
    }

    // ===== Edge Operations =====

    fn upsert_edge(
        &self,
        from_memory_id: &str,
        to_memory_id: &str,
        rel_type: &str,
        note: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i64>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_memory_id.to_string();
        let to_id = to_memory_id.to_string();
        let rel = rel_type.to_string();
        let note_val = note.map(|s| s.to_string());
        
        Box::pin(async move {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Memory', '{}', ?, ?)")
                .bind(&from_id)
                .bind(&now)
                .bind(&now)
                .execute(&pool)
                .await?;
            sqlx::query("INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, 'Memory', '{}', ?, ?)")
                .bind(&to_id)
                .bind(&now)
                .bind(&now)
                .execute(&pool)
                .await?;

            sqlx::query(
                "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(format!("{}:{}:{}", from_id, rel, to_id))
            .bind(&from_id)
            .bind(&rel)
            .bind(&to_id)
            .bind(note_val.map(|n| serde_json::json!({"note": n})).unwrap_or_else(|| serde_json::json!({})).to_string())
            .bind(&now)
            .execute(&pool)
            .await?;

            Ok(0)
        })
    }

    fn delete_edge(
        &self,
        from_memory_id: &str,
        rel_type: &str,
        to_memory_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool>> + Send + '_>> {
        let pool = self.pool.clone();
        let from_id = from_memory_id.to_string();
        let to_id = to_memory_id.to_string();
        let rel = rel_type.to_string();
        
        Box::pin(async move {
            let result = sqlx::query(
                "DELETE FROM edges WHERE from_id = ? AND to_id = ? AND edge_type = ?"
            )
            .bind(&from_id)
            .bind(&to_id)
            .bind(&rel)
            .execute(&pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
    }

    // ===== Traversal Query Operations =====

    fn get_outgoing_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String, Option<String>)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let node_id = node_id.to_string();
            let result: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT to_id, edge_type, properties FROM edges WHERE from_id = ?"
            )
            .bind(&node_id)
            .fetch_all(&pool)
            .await?;
            Ok(result.into_iter().map(|(to_id, edge_type, props)| {
                let note = serde_json::from_str::<serde_json::Value>(&props).ok().and_then(|v| v.get("note").and_then(|n| n.as_str()).map(|s| s.to_string()));
                (to_id, edge_type, note)
            }).collect())
        })
    }

    fn get_incoming_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String, Option<String>)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let node_id = node_id.to_string();
            let result: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT from_id, edge_type, properties FROM edges WHERE to_id = ?"
            )
            .bind(&node_id)
            .fetch_all(&pool)
            .await?;
            Ok(result.into_iter().map(|(from_id, edge_type, props)| {
                let note = serde_json::from_str::<serde_json::Value>(&props).ok().and_then(|v| v.get("note").and_then(|n| n.as_str()).map(|s| s.to_string()));
                (from_id, edge_type, note)
            }).collect())
        })
    }

    fn get_all_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let node_id = node_id.to_string();
            let result: Vec<(String, String)> = sqlx::query_as(
                "SELECT to_id, edge_type FROM edges WHERE from_id = ?
                 UNION
                 SELECT from_id, edge_type FROM edges WHERE to_id = ?"
            )
            .bind(&node_id)
            .bind(&node_id)
            .fetch_all(&pool)
            .await?;
            Ok(result)
        })
    }

    // ===== PageRank Data =====

    fn get_all_memory_edges(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(i64, i64)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT from_id, to_id FROM edges
                 WHERE edge_type NOT IN ('HAS_TYPE', 'HAS_CHUNK', 'HAS_TAG', 'MENTIONS')"
            )
            .fetch_all(&pool)
            .await?;

            let memory_rows: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM nodes WHERE type = 'Memory'"
            )
            .fetch_all(&pool)
            .await?;

            let mut id_map = std::collections::HashMap::new();
            for (idx, (memory_id,)) in memory_rows.into_iter().enumerate() {
                id_map.insert(memory_id, idx as i64 + 1);
            }

            Ok(rows.into_iter().filter_map(|(from_id, to_id)| {
                match (id_map.get(&from_id), id_map.get(&to_id)) {
                    (Some(&from_idx), Some(&to_idx)) => Some((from_idx, to_idx)),
                    _ => None,
                }
            }).collect())
        })
    }

    fn get_all_memory_nodes(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(i64, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM nodes WHERE type = 'Memory' ORDER BY id"
            )
            .fetch_all(&pool)
            .await?;

            Ok(rows.into_iter().enumerate().map(|(idx, (memory_id,))| {
                (idx as i64 + 1, memory_id)
            }).collect())
        })
    }

    fn get_all_concept_nodes(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result: Vec<(String,)> = sqlx::query_as(
                "SELECT id FROM ontology_concepts"
            ).fetch_all(&pool).await?;
            Ok(result.into_iter().map(|(id,)| id).collect())
        })
    }

    fn get_all_ontology_edges(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result: Vec<(String, String)> = sqlx::query_as(
                "SELECT from_id, to_id FROM ontology_edges"
            ).fetch_all(&pool).await?;
            Ok(result)
        })
    }

    fn get_graph_stats(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(i64, i64, HashMap<String, i64>)>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
                .fetch_one(&pool).await.unwrap_or(0);
            
            let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges")
                .fetch_one(&pool).await.unwrap_or(0);

            let rel_rows: Vec<(String, i64)> = sqlx::query_as(
                "SELECT edge_type, COUNT(*) FROM edges GROUP BY edge_type ORDER BY COUNT(*) DESC"
            ).fetch_all(&pool).await.unwrap_or_default();

            let mut rel_type_counts = HashMap::new();
            for (rel_type, count) in rel_rows {
                rel_type_counts.insert(rel_type, count);
            }

            Ok((node_count, edge_count, rel_type_counts))
        })
    }

    // ===== Cypher Query Execution =====

    fn execute_cypher(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<HashMap<String, Value>>>> + Send + '_>> {
        let pool = self.pool.clone();
        let sql = sql.to_string();
        let params = params.to_vec();
        
        Box::pin(async move {
            use sqlx::Row;
            use sqlx::Column;

            // Build query with dynamic binding
            let mut q = sqlx::query(&sql);
            for param in &params {
                match param {
                    serde_json::Value::String(s) => q = q.bind(s.clone()),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            q = q.bind(i);
                        } else if let Some(f) = n.as_f64() {
                            q = q.bind(f);
                        }
                    }
                    serde_json::Value::Null => q = q.bind(Option::<String>::None),
                    other => q = q.bind(other.to_string()),
                }
            }

            let rows = q.fetch_all(&pool).await?;
            let mut results = Vec::new();

            for row in rows {
                let mut map = HashMap::new();
                for (i, col) in row.columns().iter().enumerate() {
                    let val: serde_json::Value = match row.try_get::<String, _>(i) {
                        Ok(s) => serde_json::Value::String(s),
                        Err(_) => match row.try_get::<i64, _>(i) {
                            Ok(n) => serde_json::Value::Number(n.into()),
                            Err(_) => match row.try_get::<f64, _>(i) {
                                Ok(f) => serde_json::json!(f),
                                Err(_) => serde_json::Value::Null,
                            }
                        }
                    };
                    map.insert(col.name().to_string(), val);
                }
                results.push(map);
            }

            Ok(results)
        })
    }
}
