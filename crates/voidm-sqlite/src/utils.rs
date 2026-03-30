//! Utility re-exports and backend-specific functions for voidm-sqlite
//! 
//! These utilities are backend-specific operations that use sqlx directly.
//! They should NOT be in voidm-core but in the backend implementation.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

// Re-export core utilities used by add_memory_backend
pub use voidm_core::embeddings::embed_text_chunked;
pub use voidm_core::vector::ensure_vector_table;
pub use voidm_core::crud::convert_memory_type;
pub use voidm_core::crud::redact_memory;

// Re-export types
pub use voidm_core::Config;

/// Resolve a memory ID (from short prefix or full UUID) - BACKEND UTILITY
/// This function uses sqlx and must stay in the backend, not in core
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
