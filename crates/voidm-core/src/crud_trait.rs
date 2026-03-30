//! Trait-based CRUD operations - backend-agnostic versions
//! 
//! These functions use the Database trait instead of SqlitePool,
//! making them work with any backend (SQLite, PostgreSQL, Neo4j, etc.)
//!
//! Over time, these will replace the SqlitePool-based versions in crud.rs

use anyhow::Result;
use std::sync::Arc;
use voidm_db::Database;
use crate::models::{AddMemoryRequest, AddMemoryResponse, Memory};
use crate::Config;

/// Get a memory by ID using trait-based backend
pub async fn get_memory(
    db: &Arc<dyn Database>,
    id: &str,
) -> Result<Option<Memory>> {
    if let Some(memory_json) = db.get_memory(id).await? {
        let memory: Memory = serde_json::from_value(memory_json)?;
        Ok(Some(memory))
    } else {
        Ok(None)
    }
}

/// List memories using trait-based backend
pub async fn list_memories(
    db: &Arc<dyn Database>,
    limit: Option<usize>,
) -> Result<Vec<Memory>> {
    let memories_json = db.list_memories(limit).await?;
    let mut memories = Vec::new();
    for memory_json in memories_json {
        let memory: Memory = serde_json::from_value(memory_json)?;
        memories.push(memory);
    }
    Ok(memories)
}

/// Delete a memory using trait-based backend
pub async fn delete_memory(
    db: &Arc<dyn Database>,
    id: &str,
) -> Result<bool> {
    db.delete_memory(id).await
}

/// Add a memory using trait-based backend
pub async fn add_memory(
    db: &Arc<dyn Database>,
    req: AddMemoryRequest,
    config: &Config,
) -> Result<AddMemoryResponse> {
    let req_json = serde_json::to_value(req)?;
    let config_json = serde_json::to_value(config)?;
    let response_json = db.add_memory(req_json, &config_json).await?;
    let response: AddMemoryResponse = serde_json::from_value(response_json)?;
    Ok(response)
}

/// Resolve a memory ID (full or short prefix) using trait-based backend
pub async fn resolve_memory_id(
    db: &Arc<dyn Database>,
    id: &str,
) -> Result<String> {
    db.resolve_memory_id(id).await
}

/// Check for embedding model mismatch using trait-based backend
pub async fn check_model_mismatch(
    db: &Arc<dyn Database>,
    configured_model: &str,
) -> Result<Option<(String, String)>> {
    db.check_model_mismatch(configured_model).await
}

/// Link two memories using trait-based backend
pub async fn link_memories(
    db: &Arc<dyn Database>,
    from_id: &str,
    rel: &str,
    to_id: &str,
    note: Option<&str>,
) -> Result<serde_json::Value> {
    db.link_memories(from_id, rel, to_id, note).await
}

/// Unlink two memories using trait-based backend
pub async fn unlink_memories(
    db: &Arc<dyn Database>,
    from_id: &str,
    rel: &str,
    to_id: &str,
) -> Result<bool> {
    db.unlink_memories(from_id, rel, to_id).await
}

/// List memories with optional scope and type filtering using trait-based backend
pub async fn list_memories_filtered(
    db: &Arc<dyn Database>,
    scope_filter: Option<&str>,
    type_filter: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<Memory>> {
    let memories = list_memories(db, limit).await?;
    let filtered: Vec<Memory> = memories.into_iter()
        .filter(|m| {
            if let Some(scope) = scope_filter {
                if !m.scopes.iter().any(|s| s.contains(scope)) {
                    return false;
                }
            }
            if let Some(mtype) = type_filter {
                if m.memory_type != mtype {
                    return false;
                }
            }
            true
        })
        .collect();
    Ok(filtered)
}

/// List all scopes using trait-based backend
pub async fn list_scopes(
    db: &Arc<dyn Database>,
) -> Result<Vec<String>> {
    db.list_scopes().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trait_crud_functions_exist() {
        // This test just verifies the functions compile and are callable
        // Integration tests would use a real database backend
    }
}
