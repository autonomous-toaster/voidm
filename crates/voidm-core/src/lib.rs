// Simple compatibility type for DbPool
pub type DbPool = std::sync::Arc<dyn voidm_db_trait::Database>;

pub mod config;
pub mod crud;
pub mod crud_logic;
pub mod crud_trait;
pub mod query;
pub mod migrate;
pub mod models;
pub mod search;
pub mod migration;
pub mod graph_retrieval;
pub mod rrf_fusion;
pub mod fast_vector;
pub mod vector; // Deprecated: kept for compatibility only
pub mod query_classifier;
pub mod context_boosting;
pub mod importance_boosting;
pub mod quality_filtering;
pub mod recency_boosting;
pub mod migration_export;
pub mod vector_format;
pub mod db_migration;
pub mod chunking;
pub mod validation;
pub mod coherence;
pub mod similarity;
pub mod neo4j_schema;
pub mod neo4j_db;
pub mod export;
pub mod import;
pub mod chunk_nodes; // Phase 0.5.3: Generic node/edge storage for chunks

pub use config::Config;
pub use config::config_path_display;
pub use validation::validate_memory_length;
pub use chunking::{chunk_smart, ChunkingStrategy, Chunk, BreakType};
pub use coherence::estimate_coherence;
pub use neo4j_db::Neo4jDb;

// Re-export from separate crates
#[cfg(feature = "ner")]
pub use voidm_ner as ner;
pub use voidm_redactor as redactor;
#[cfg(feature = "reranker")]
pub use voidm_reranker as reranker;
pub use voidm_embeddings as embeddings;
pub use voidm_scoring as quality;
#[cfg(feature = "query-expansion")]
pub use voidm_query_expansion as query_expansion;

pub use crud::{resolve_id, resolve_id_sqlite};
pub use models::{Memory, MemoryType, AddMemoryRequest, AddMemoryResponse, SuggestedLink, DuplicateWarning, MemoryEdge, validate_title};
pub use migration_export::{VectorBackup, MigrationCheckpoint};
pub use neo4j_schema::{MemoryChunkSchema, SchemaStats, CoherenceStats};
