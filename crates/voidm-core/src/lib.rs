pub mod config;
pub mod crud;
pub mod db;
pub mod migrate;
pub mod models;
pub mod ner;
pub mod nli;
pub mod ontology;
pub mod quality;
pub mod vector;
pub mod search;
pub mod embeddings;
pub mod migration;
pub mod concept_extraction;
pub mod concept_clustering;
pub mod concept_deduplication;
pub mod concept_linking;
pub mod concept_hierarchy;
pub mod concept_ranking;
pub mod multi_relation_detection;
pub mod concept_telemetry;
pub mod improvement_engine;
pub mod agent_feedback;

pub use config::Config;
pub use config::config_path_display;
pub use db::sqlite::open_pool;  // Re-export for backward compatibility
pub use crud::resolve_id;
pub use models::{Memory, MemoryType, AddMemoryRequest, AddMemoryResponse, SuggestedLink, DuplicateWarning, MemoryEdge, OntologyEdgeForMigration};
