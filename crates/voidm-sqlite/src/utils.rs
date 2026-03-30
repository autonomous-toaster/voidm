//! Utility re-exports for voidm-sqlite
//! 
//! These utilities are imported from voidm-core but re-exported here
//! to maintain clean import boundaries. Eventually these should be moved
//! to voidm-db for true backend independence.
//!
//! TODO (Phase 1.6+): Move these to voidm-db foundation layer

// Re-export core utilities used by add_memory_backend
pub use voidm_core::embeddings::embed_text_chunked;
pub use voidm_core::vector::ensure_vector_table;
pub use voidm_core::crud::convert_memory_type;
pub use voidm_core::crud::redact_memory;
pub use voidm_core::crud::resolve_id_sqlite;

// Re-export types
pub use voidm_core::Config;
