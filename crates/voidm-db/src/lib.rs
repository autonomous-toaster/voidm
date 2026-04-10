//! voidm-db: Database abstraction, models, and configuration
//!
//! This crate is the foundation for voidm's backend-independent architecture:
//! - `Database` trait: abstraction for all backends (SQLite, PostgreSQL, Neo4j, etc.)
//! - `models`: shared data structures used throughout voidm
//! - `Config`: global application configuration
//!
//! This crate has minimal dependencies to avoid coupling backend implementations
//! to specific internal types.
//!
//! # Design Philosophy
//!
//! This trait uses `serde_json::Value` for complex types to decouple from
//! voidm-core's SQLx-annotated types. Each backend implementation handles
//! its own type conversions at the boundary.

pub mod models;
pub mod graph_ops;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::future::Future;

/// Result of memory ID resolution (exact match or prefix match)
/// 
/// When resolving a memory ID, it can either match a single memory
/// (exact match or unique prefix match) or multiple memories (prefix match).
/// This enum allows callers to handle both cases appropriately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolveResult {
    /// Exact match or single prefix match (safe to auto-use)
    Single(String),
    /// Multiple prefix matches (requires user choice/confirmation)
    Multiple(Vec<String>),
}

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
    ///
    /// # Behavior
    /// - **Exact match first:** If `id` exactly matches a memory, return it regardless of length
    /// - **Prefix match:** If not exact, try prefix match with minimum 8 characters
    /// - **Single match:** If prefix matches exactly 1 memory, return `Single(id)`
    /// - **Multiple matches:** If prefix matches 2+ memories, return `Multiple(ids)` for bulk operations
    /// - **Error cases:**
    ///   - Prefix < 8 chars and no exact match: "too short" error
    ///   - No matches found: "not found" error
    ///
    /// # Examples
    /// ```ignore
    /// // Exact match (any length)
    /// resolve_memory_id("mem_abc1234567890abcd") → Single("mem_abc1234567890abcd")
    ///
    /// // Prefix match (8+ chars)
    /// resolve_memory_id("mem_test_prefix") → Single(...) or Multiple([...])  
    ///
    /// // Prefix too short
    /// resolve_memory_id("mem_abc") → Error("too short")
    /// ```
    fn resolve_memory_id(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<ResolveResult>> + Send + '_>>;

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

    /// BM25 full-text search over memory content
    /// Returns ranked results as (id, normalized_score) tuples
    fn search_bm25(
        &self,
        query: &str,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>>;

    /// BM25-style lexical search over titles
    /// Returns ranked results as (memory_id, normalized_score) tuples
    fn search_title_bm25(
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

    /// Vector ANN search using memory-level embeddings
    /// Returns ranked results as (id, similarity_score) tuples where score is in [0,1]
    fn search_ann(
        &self,
        embedding: Vec<f32>,
        limit: usize,
        scope_filter: Option<&str>,
        type_filter: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>>;

    /// Vector ANN search using chunk-level embeddings
    /// Returns ranked results as (chunk_id, similarity_score) tuples where score is in [0,1]
    fn search_chunk_ann(
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

    /// Fetch memories with timestamps for chunking
    /// Returns: (id, content, created_at)
    fn fetch_memories_for_chunking(
        &self,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>>;

    // ===== Graph Operations =====

    /// Execute a Cypher query (read-only)
    fn query_cypher(
        &self,
        query: &str,
        params: &serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    /// Get neighbors of a node at specified depth
    fn get_neighbors(&self, id: &str, depth: usize) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + '_>>;

    // ===== Statistics & Reporting =====

    /// Get comprehensive database statistics
    /// Returns: total memories, memories by type, scopes count, top tags, graph stats, embeddings coverage, db size
    fn get_statistics(&self) -> Pin<Box<dyn Future<Output = Result<models::DatabaseStats>> + Send + '_>>;

    /// Get graph statistics only (nodes, edges, edges by type)
    fn get_graph_stats(&self) -> Pin<Box<dyn Future<Output = Result<models::GraphStats>> + Send + '_>>;

    /// Get graph export data (memories, concepts, edges for export)
    fn get_graph_export_data(&self) -> Pin<Box<dyn Future<Output = Result<models::GraphExportData>> + Send + '_>>;

    // ===== Utility =====

    /// Check if embedding model in database matches configured model
    fn check_model_mismatch(
        &self,
        configured_model: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, String)>>> + Send + '_>>;

    /// Clean the database by removing all known node types and edges
    /// Only implemented for Neo4j. SQLite backends can safely ignore this.
    /// Useful when re-running migrations to avoid constraint violations.
    /// Returns: count of deleted items
    fn clean_database(&self) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>> {
        // Default no-op implementation for SQLite and other backends
        Box::pin(async { Ok(0) })
    }

    /// Perform backend-specific shutdown operations (e.g., SQLite WAL checkpoints)
    /// Default no-op for backends that don't need special shutdown handling
    fn shutdown(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }

    /// Delete all chunks for a memory (used when memory content is updated)
    /// Chunk immutability: old chunks are deleted, new ones recreated with new timestamp
    fn delete_chunks_for_memory(
        &self,
        memory_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>>;

    /// Fetch all chunks (or limited set) for embedding generation
    fn fetch_chunks(
        &self,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, String, String)>>> + Send + '_>>;

    /// Upsert a chunk and attach it to its owning memory.
    fn upsert_chunk(
        &self,
        chunk_id: &str,
        memory_id: &str,
        content: &str,
        index: usize,
        size: usize,
        created_at: &str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Store embedding for a chunk
    /// Returns: (chunk_id, embedding_dimension)
    fn store_chunk_embedding(
        &self,
        chunk_id: String,
        memory_id: String,
        embedding: Vec<f32>,
    ) -> Pin<Box<dyn Future<Output = Result<(String, usize)>> + Send + '_>>;

    /// Get embedding for a specific chunk
    /// Returns: Option<embedding_vector>
    fn get_chunk_embedding(
        &self,
        chunk_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<f32>>>> + Send + '_>>;

    /// Search chunks by embedding similarity
    /// Returns: Vec<(chunk_id, similarity_score)> ordered by score descending
    fn search_by_embedding(
        &self,
        query_embedding: Vec<f32>,
        limit: usize,
        min_similarity: f32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<(String, f32)>>> + Send + '_>>;

    /// Export all memories and chunks to JSONL format
    fn export_to_jsonl(
        &self,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;

    /// Import memories and chunks from JSONL format
    fn import_from_jsonl(
        &self,
        records: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<(usize, usize, usize)>> + Send + '_>>;

    // ===== Tags (NEW - for migration) =====

    /// List all tags (with their properties)
    fn list_tags(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// Create a tag (idempotent - returns existing if already present)
    /// Returns: (tag_id, created: bool)
    fn create_tag(&self, name: &str) -> Pin<Box<dyn Future<Output = Result<(String, bool)>> + Send + '_>>;

    /// Link a memory to a tag
    fn link_tag_to_memory(&self, tag_id: &str, memory_id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// List all tag-memory edges (for migration)
    fn list_tag_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    // ===== Chunks (NEW - for migration) =====

    /// List all chunks with full details (for migration)
    fn list_chunks(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// Get a specific chunk by ID (for migration)
    fn get_chunk(&self, chunk_id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>>;

    /// List all chunk-memory BELONGS_TO edges (for migration)
    fn list_chunk_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    // ===== Entities (NEW - for migration) =====

    /// List all entities (for migration)
    fn list_entities(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// Create an entity node (idempotent - returns existing if already present)
    /// Returns: (entity_id, created: bool)
    fn get_or_create_entity(&self, name: &str, entity_type: &str) -> Pin<Box<dyn Future<Output = Result<(String, bool)>> + Send + '_>>;

    /// Link a chunk to an entity with confidence score
    fn link_chunk_to_entity(
        &self,
        chunk_id: &str,
        entity_id: &str,
        confidence: f32,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// List all chunk-entity MENTIONS edges (for migration)
    fn list_entity_mention_edges(&self) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// Count of a specific node type (for validation)
    /// Node types: Memory, MemoryChunk, Tag, Entity, Concept
    fn count_nodes(&self, node_type: &str) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>>;

    /// Count of a specific edge type (for validation)
    /// Edge types: BELONGS_TO, HAS_TAG, MENTIONS, all types
    fn count_edges(&self, edge_type: Option<&str>) -> Pin<Box<dyn Future<Output = Result<usize>> + Send + '_>>;

    // ===== Generic Node/Edge API (Phase 0) =====

    /// Create a node in generic format (id, type, properties JSON)
    fn create_node(
        &self,
        id: &str,
        node_type: &str,
        properties: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Get a node by ID
    fn get_node(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>>;

    /// Delete a node by ID
    fn delete_node(&self, id: &str) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// List all nodes of a specific type
    fn list_nodes(&self, node_type: &str) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    /// Create an edge (from_id, edge_type, to_id, properties JSON)
    fn create_edge(
        &self,
        from_id: &str,
        edge_type: &str,
        to_id: &str,
        properties: Option<serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Get an edge (returns full edge object with id, from_id, edge_type, to_id, properties)
    fn get_edge(
        &self,
        from_id: &str,
        edge_type: &str,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>>> + Send + '_>>;

    /// Delete an edge
    fn delete_edge(
        &self,
        from_id: &str,
        edge_type: &str,
        to_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + '_>>;

    /// Get all edges from a node, optionally filtered by edge_type
    fn get_node_edges(
        &self,
        node_id: &str,
        edge_type: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>;

    // ===== Graph Operations =====

    /// Get graph query operations for this backend
    /// Returns a trait object that implements GraphQueryOps
    fn graph_ops(&self) -> std::sync::Arc<dyn crate::graph_ops::GraphQueryOps>;
}
