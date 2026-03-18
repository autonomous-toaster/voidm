pub mod config;
pub mod crud;
pub mod db;
pub mod query;
pub mod migrate;
pub mod models;
#[cfg(feature = "nli")]
pub mod nli;
pub mod ontology;
pub mod vector;
pub mod search;
pub mod migration;
#[cfg(feature = "tinyllama")]
pub mod auto_tagger_tinyllama;
pub mod graph_retrieval;
pub mod rrf_fusion;

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

pub use db::sqlite::open_pool;  // Re-export for backward compatibility
pub use crud::{resolve_id, resolve_id_sqlite};
pub use models::{Memory, MemoryType, AddMemoryRequest, AddMemoryResponse, SuggestedLink, DuplicateWarning, MemoryEdge, OntologyEdgeForMigration};
