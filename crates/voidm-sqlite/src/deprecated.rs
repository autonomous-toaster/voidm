//! Deprecated vector functions - kept for backward compatibility only
//! These are stubs that do nothing. Use Database trait methods instead.

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
