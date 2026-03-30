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
            sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
                .bind(&memory_id)
                .execute(&pool)
                .await?;
            let id: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
                .bind(&memory_id)
                .fetch_one(&pool)
                .await?;
            Ok(id)
        })
    }

    fn delete_node(&self, memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        let pool = self.pool.clone();
        let memory_id = memory_id.to_string();
        Box::pin(async move {
            sqlx::query("DELETE FROM graph_nodes WHERE memory_id = ?")
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
            let id: Option<i64> = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
                .bind(&memory_id)
                .fetch_optional(&pool)
                .await?;
            Ok(id)
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
            // Upsert nodes first using separate operations
            sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
                .bind(&from_id)
                .execute(&pool)
                .await?;
            
            let from_node: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
                .bind(&from_id)
                .fetch_one(&pool)
                .await?;

            sqlx::query("INSERT OR IGNORE INTO graph_nodes (memory_id) VALUES (?)")
                .bind(&to_id)
                .execute(&pool)
                .await?;
            
            let to_node: i64 = sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
                .bind(&to_id)
                .fetch_one(&pool)
                .await?;

            let now = chrono::Utc::now().to_rfc3339();
            
            sqlx::query(
                "INSERT OR IGNORE INTO graph_edges (source_id, target_id, rel_type, note, created_at)
                 VALUES (?, ?, ?, ?, ?)"
            )
            .bind(from_node)
            .bind(to_node)
            .bind(&rel)
            .bind(&note_val)
            .bind(&now)
            .execute(&pool)
            .await?;

            let edge_id: i64 = sqlx::query_scalar(
                "SELECT id FROM graph_edges WHERE source_id = ? AND target_id = ? AND rel_type = ?"
            )
            .bind(from_node)
            .bind(to_node)
            .bind(&rel)
            .fetch_one(&pool)
            .await?;

            Ok(edge_id)
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
            let from_node: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM graph_nodes WHERE memory_id = ?"
            )
            .bind(&from_id)
            .fetch_optional(&pool)
            .await?;

            let to_node: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM graph_nodes WHERE memory_id = ?"
            )
            .bind(&to_id)
            .fetch_optional(&pool)
            .await?;

            match (from_node, to_node) {
                (Some(f), Some(t)) => {
                    let result = sqlx::query(
                        "DELETE FROM graph_edges WHERE source_id = ? AND target_id = ? AND rel_type = ?"
                    )
                    .bind(f)
                    .bind(t)
                    .bind(&rel)
                    .execute(&pool)
                    .await?;
                    Ok(result.rows_affected() > 0)
                }
                _ => Ok(false),
            }
        })
    }

    // ===== Traversal Query Operations =====

    fn get_outgoing_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String, Option<String>)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result: Vec<(String, String, Option<String>)> = sqlx::query_as(
                "SELECT n.memory_id, e.rel_type, e.note
                 FROM graph_edges e
                 JOIN graph_nodes n ON n.id = e.target_id
                 WHERE e.source_id = ?"
            )
            .bind(node_id)
            .fetch_all(&pool)
            .await?;
            Ok(result)
        })
    }

    fn get_incoming_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String, Option<String>)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result: Vec<(String, String, Option<String>)> = sqlx::query_as(
                "SELECT n.memory_id, e.rel_type, e.note
                 FROM graph_edges e
                 JOIN graph_nodes n ON n.id = e.source_id
                 WHERE e.target_id = ?"
            )
            .bind(node_id)
            .fetch_all(&pool)
            .await?;
            Ok(result)
        })
    }

    fn get_all_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result: Vec<(String, String)> = sqlx::query_as(
                "SELECT n.memory_id, e.rel_type FROM graph_edges e
                 JOIN graph_nodes n ON n.id = e.target_id WHERE e.source_id = ?
                 UNION
                 SELECT n.memory_id, e.rel_type FROM graph_edges e
                 JOIN graph_nodes n ON n.id = e.source_id WHERE e.target_id = ?"
            )
            .bind(node_id)
            .bind(node_id)
            .fetch_all(&pool)
            .await?;
            Ok(result)
        })
    }

    // ===== PageRank Data =====

    fn get_all_memory_edges(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(i64, i64)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result: Vec<(i64, i64)> = sqlx::query_as(
                "SELECT source_id, target_id FROM graph_edges"
            ).fetch_all(&pool).await?;
            Ok(result)
        })
    }

    fn get_all_memory_nodes(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(i64, String)>>> + Send + '_>> {
        let pool = self.pool.clone();
        Box::pin(async move {
            let result: Vec<(i64, String)> = sqlx::query_as(
                "SELECT id, memory_id FROM graph_nodes"
            ).fetch_all(&pool).await?;
            Ok(result)
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
            let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
                .fetch_one(&pool).await.unwrap_or(0);
            
            let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
                .fetch_one(&pool).await.unwrap_or(0);

            let rel_rows: Vec<(String, i64)> = sqlx::query_as(
                "SELECT rel_type, COUNT(*) FROM graph_edges GROUP BY rel_type ORDER BY COUNT(*) DESC"
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
