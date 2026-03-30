//! GraphOps trait: Backend-specific graph query operations
//!
//! These methods encapsulate the low-level queries needed by voidm-graph.
//! Implementations handle database-specific syntax and connection pooling.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

/// Graph query operations trait
///
/// Implementations are responsible for:
/// - Database connection/pooling
/// - SQL generation and parameter binding
/// - Result mapping and error handling
pub trait GraphQueryOps: Send + Sync {
    // ===== Node Operations =====

    /// Upsert a graph node for a memory_id
    /// Returns: node_id (i64)
    fn upsert_node(&self, memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i64>> + Send + '_>>;

    /// Delete a graph node by memory_id (cascades to edges via FK)
    fn delete_node(&self, memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    /// Get node ID by memory_id
    /// Returns: Option<node_id>
    fn get_node_id(&self, memory_id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<i64>>> + Send + '_>>;

    // ===== Edge Operations =====

    /// Upsert an edge between two memories
    /// Returns: edge_id (i64)
    fn upsert_edge(
        &self,
        from_memory_id: &str,
        to_memory_id: &str,
        rel_type: &str,
        note: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i64>> + Send + '_>>;

    /// Delete an edge between two memories
    /// Returns: bool (true if deleted)
    fn delete_edge(
        &self,
        from_memory_id: &str,
        rel_type: &str,
        to_memory_id: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool>> + Send + '_>>;

    // ===== Traversal Query Operations =====

    /// Get outgoing edges from a node
    /// Returns: Vec<(target_memory_id, rel_type, note)>
    fn get_outgoing_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String, Option<String>)>>> + Send + '_>>;

    /// Get incoming edges to a node
    /// Returns: Vec<(source_memory_id, rel_type, note)>
    fn get_incoming_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String, Option<String>)>>> + Send + '_>>;

    /// Get all edges (both directions) from/to a node
    /// Returns: Vec<(target_memory_id, rel_type)>
    fn get_all_edges(&self, node_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>>> + Send + '_>>;

    // ===== PageRank Data =====

    /// Get all memory edges for PageRank
    /// Returns: Vec<(source_node_id, target_node_id)>
    fn get_all_memory_edges(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(i64, i64)>>> + Send + '_>>;

    /// Get all memory nodes for PageRank
    /// Returns: Vec<(node_id, memory_id)>
    fn get_all_memory_nodes(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(i64, String)>>> + Send + '_>>;

    /// Get all concept nodes
    /// Returns: Vec<concept_id>
    fn get_all_concept_nodes(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + '_>>;

    /// Get all ontology edges
    /// Returns: Vec<(from_id, to_id)>
    fn get_all_ontology_edges(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>>> + Send + '_>>;

    /// Get graph statistics
    /// Returns: (node_count, edge_count, rel_type_counts)
    fn get_graph_stats(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(i64, i64, HashMap<String, i64>)>> + Send + '_>>;

    // ===== Cypher Query Execution =====

    /// Execute a Cypher query translated to SQL
    /// Returns: Vec<HashMap<column_name, value>>
    fn execute_cypher(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<HashMap<String, Value>>>> + Send + '_>>;
}

