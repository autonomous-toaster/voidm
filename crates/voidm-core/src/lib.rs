pub mod config;
pub mod crud;
pub mod db;
pub mod migrate;
pub mod models;
pub mod ner;
pub mod nli;
pub mod reranker;
pub mod ontology;
pub mod quality;
pub mod vector;
pub mod search;
pub mod embeddings;
pub mod migration;
pub mod semantic_dedup;
pub mod query_expansion;
pub mod gguf_query_expander;
pub mod auto_tagger;
pub mod tag_linker;
pub mod redactor;
pub mod graph_retrieval;

pub use config::Config;
pub use config::config_path_display;
pub use db::sqlite::open_pool;  // Re-export for backward compatibility
pub use crud::resolve_id;
pub use models::{Memory, MemoryType, AddMemoryRequest, AddMemoryResponse, SuggestedLink, DuplicateWarning, MemoryEdge, OntologyEdgeForMigration};
