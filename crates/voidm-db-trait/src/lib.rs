//! voidm-db-trait: Database abstraction trait for multiple backend support
//!
//! This crate defines the minimal `Database` trait needed for implementing
//! different database backends (SQLite, PostgreSQL, Neo4j, etc.).
//!
//! It has minimal dependencies to avoid coupling backend implementations to
//! specific voidm-core types.
//!
//! # Design Philosophy
//!
//! This trait uses `serde_json::Value` for complex types to decouple from
//! voidm-core's SQLx-annotated types. Each backend implementation handles
//! its own type conversions at the boundary.

use anyhow::Result;
use std::pin::Pin;
use std::future::Future;

/// Database abstraction trait for supporting multiple backends
///
/// All methods are async and return `Result<T>`. Implementations must be Send + Sync
/// to work with async runtime.
pub trait Database: Send + Sync {
    // ===== Lifecycle =====

    /// Check if the database connection is healthy
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Close the database connection cleanly
    fn close(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Ensure the database schema is initialized
    fn ensure_schema(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    // ===== Memory CRUD =====

    /// Add a new memory (request and response as JSON)
    fn add_memory(
        &self,
        req_json: serde_json::Value,
        config: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// Get a memory by ID (full ID or short prefix)
    fn get_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>>;

    /// List memories with optional limit
    fn list_memories(&self, limit: Option<usize>) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// Delete a memory by ID
    fn delete_memory(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// Update memory content (for re-embedding, etc)
    fn update_memory(
        &self,
        id: &str,
        content: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Resolve a memory ID (from short prefix or full UUID)
    fn resolve_memory_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>>;

    /// List all scopes used in memories
    fn list_scopes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;

    // ===== Memory Edges/Links =====

    /// Create a link between two memories
    fn link_memories(
        &self,
        from_id: &str,
        rel: &str,
        to_id: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// Remove a link between two memories
    fn unlink_memories(
        &self,
        from_id: &str,
        rel: &str,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// List all memory-to-memory edges (for migration)
    fn list_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// List all ontology edges (concept-to-concept, concept-to-memory, etc.)
    fn list_ontology_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// Create an ontology edge (for migration)
    fn create_ontology_edge(
        &self,
        from_id: &str,
        from_type: &str,
        rel_type: &str,
        to_id: &str,
        to_type: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    // ===== Search =====

    /// Hybrid search (vector + BM25 + fuzzy)
    fn search_hybrid(
        &self,
        opts_json: serde_json::Value,
        model_name: &str,
        embeddings_enabled: bool,
        config_min_score: f32,
        config_search: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// BM25 full-text search (backend-specific implementation)
    /// Returns ranked results as (id, normalized_score) tuples
    fn search_bm25(
        &self,
        query: &str,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>>;

    /// Fuzzy/similarity search using Jaro-Winkler distance
    /// Returns ranked results as (id, similarity_score) tuples where score is in [0,1]
    fn search_fuzzy(
        &self,
        query: &str,
        scope_filter: Option<&str>,
        limit: usize,
        threshold: f32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>>;

    /// Vector ANN search using sqlite-vec
    /// Returns ranked results as (id, similarity_score) tuples where score is in [0,1]
    fn search_ann(
        &self,
        embedding: Vec<f32>,
        limit: usize,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>>;

    /// Fetch raw memories for custom scoring
    /// Returns (id, content) tuples ordered by creation timestamp (newest first)
    fn fetch_memories_raw(
        &self,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String)>>> + Send + '_>>;

    // ===== Ontology Concepts =====

    /// Create a new concept
    fn add_concept(
        &self,
        name: &str,
        description: Option<&str>,
        scope: Option<&str>,
        id: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// Get a concept by ID
    fn get_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// Get a concept with its instances, subclasses, and superclasses
    fn get_concept_with_instances(
        &self,
        id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// List concepts with optional scope filter
    fn list_concepts(
        &self,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// Delete a concept
    fn delete_concept(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// Resolve a concept ID (from short prefix or full UUID)
    fn resolve_concept_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>>;

    /// Search for concepts by name and description
    fn search_concepts(
        &self,
        query: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    // ===== Ontology Edges =====

    /// Create an ontology edge
    fn add_ontology_edge(
        &self,
        from_id: &str,
        from_kind: &str,
        rel: &str,
        to_id: &str,
        to_kind: &str,
        note: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// Delete an ontology edge by ID
    fn delete_ontology_edge(&self, edge_id: i64) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    // ===== Graph Operations =====

    /// Execute a Cypher query (read-only)
    fn query_cypher(
        &self,
        query: &str,
        params: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// Get neighbors of a node at specified depth
    fn get_neighbors(&self, id: &str, depth: usize) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    // ===== Utility =====

    /// Check if embedding model in database matches configured model
    fn check_model_mismatch(
        &self,
        configured_model: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>>;

    /// Clean the database by removing all Concept and OntologyEdge nodes
    /// Only implemented for Neo4j. SQLite backends can safely ignore this.
    /// Useful when re-running migrations to avoid constraint violations.
    fn clean_database(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        // Default no-op implementation for SQLite and other backends
        Box::pin(async { Ok(()) })
    }
}
