//! Backend-specific vector operations
//! 
//! NOTE: This module is deprecated and kept for compatibility only.
//! Vector/embedding operations should be handled through the Database trait
//! in backend-specific crates (voidm-sqlite, voidm-neo4j, etc.).
//! 
//! These functions are only called from legacy code and will be removed
//! as the codebase fully adopts trait-based database access.

use anyhow::{Result, bail};
use sqlx::SqlitePool;

/// Deprecated: Use Database trait implementation instead
pub async fn ensure_vector_table(_pool: &SqlitePool, _dim: usize) -> Result<()> {
    // Vector table creation is now handled by backend implementations
    // This is kept for backwards compatibility but is a no-op
    Ok(())
}

/// Deprecated: Use Database trait implementation instead
pub async fn vec_table_exists(_pool: &SqlitePool) -> Result<bool> {
    // Check moved to backend implementations
    Ok(false)
}

/// Deprecated: Use Database trait implementation instead
pub async fn ann_search(_pool: &SqlitePool, _query_embedding: &[f32], _limit: usize) -> Result<Vec<(String, f32)>> {
    // ANN search should be called through Database trait
    bail!("ANN search should use Database trait implementation")
}

/// Deprecated: Use Database trait implementation instead
pub async fn reembed_all(
    _pool: &SqlitePool,
    _model_name: &str,
    _new_dim: usize,
    _batch_size: usize,
) -> Result<()> {
    // Re-embedding should be handled by backend implementations
    bail!("Re-embedding should use Database trait implementation")
}

/// Deprecated: Use Database trait implementation instead
pub async fn cleanup_stale_temp_table(_pool: &SqlitePool) -> Result<()> {
    // Cleanup is now handled by backend implementations
    Ok(())
}
