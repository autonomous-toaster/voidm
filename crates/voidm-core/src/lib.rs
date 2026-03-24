// Simple compatibility type for DbPool
pub type DbPool = std::sync::Arc<dyn voidm_db_trait::Database>;

pub mod config;
pub mod crud;
pub mod crud_trait;
pub mod query;
pub mod migrate;
pub mod models;
pub mod ontology;
pub mod search;
pub mod migration;
pub mod graph_retrieval;
pub mod rrf_fusion;
pub mod fast_vector;
pub mod vector; // Deprecated: kept for compatibility only
pub mod query_classifier;

pub use config::Config;
pub use config::config_path_display;

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
pub use models::{Memory, MemoryType, AddMemoryRequest, AddMemoryResponse, SuggestedLink, DuplicateWarning, MemoryEdge, OntologyEdgeForMigration};
