use anyhow::Result;
use voidm_db::graph_ops::GraphQueryOps;

/// Get or create a graph node for a memory_id.
pub async fn upsert_node(ops: &dyn GraphQueryOps, memory_id: &str) -> Result<i64> {
    ops.upsert_node(memory_id).await
}

/// Delete a graph node and all its edges (cascade via FK).
pub async fn delete_node(ops: &dyn GraphQueryOps, memory_id: &str) -> Result<()> {
    ops.delete_node(memory_id).await
}

/// Create an edge between two memory_ids. Returns the edge id.
pub async fn upsert_edge(
    ops: &dyn GraphQueryOps,
    from_memory_id: &str,
    to_memory_id: &str,
    rel_type: &str,
    note: Option<&str>,
) -> Result<i64> {
    ops.upsert_edge(from_memory_id, to_memory_id, rel_type, note).await
}

/// Delete a specific edge between two memories.
pub async fn delete_edge(
    ops: &dyn GraphQueryOps,
    from_memory_id: &str,
    rel_type: &str,
    to_memory_id: &str,
) -> Result<bool> {
    ops.delete_edge(from_memory_id, rel_type, to_memory_id).await
}
