/// Neo4j schema creation for Phase A Part D (MemoryChunk nodes)
///
/// This module handles:
/// - Defining the MemoryChunk schema structure
/// - Providing Cypher queries for schema creation
/// - Providing query patterns for chunking operations
///
/// NOTE: This module provides schema definitions and Cypher queries.
/// Actual Neo4j connection and execution happens in Part D during chunking.
///
/// Usage in Part D:
/// ```ignore
/// let schema = MemoryChunkSchema::new();
/// let constraint_query = schema.create_chunk_constraint();
/// graph.execute(constraint_query).await?;
/// ```

use anyhow::Result;
use std::collections::HashMap;

/// Neo4j schema definition for MemoryChunk
pub struct MemoryChunkSchema;

impl MemoryChunkSchema {
    /// Get Cypher query to create UNIQUE constraint on MemoryChunk.id
    pub fn create_chunk_id_constraint() -> &'static str {
        "CREATE CONSTRAINT chunk_id_unique IF NOT EXISTS \
         FOR (c:MemoryChunk) REQUIRE c.id IS UNIQUE"
    }

    /// Get Cypher query to create index on MemoryChunk.memory_id
    pub fn create_memory_id_index() -> &'static str {
        "CREATE INDEX chunk_memory_id_idx IF NOT EXISTS \
         FOR (c:MemoryChunk) ON c.memory_id"
    }

    /// Get Cypher query to create index on MemoryChunk.created_at
    pub fn create_created_at_index() -> &'static str {
        "CREATE INDEX chunk_created_at_idx IF NOT EXISTS \
         FOR (c:MemoryChunk) ON c.created_at"
    }

    /// Get Cypher query to create index on Memory.id
    pub fn create_memory_id_index_on_memory() -> &'static str {
        "CREATE INDEX memory_id_idx IF NOT EXISTS \
         FOR (m:Memory) ON m.id"
    }

    /// Create a MemoryChunk node
    /// 
    /// Parameters:
    /// - chunk_id: String (e.g., "mchk_7c95c444adb855ea8fac53150ae3fed1")
    /// - memory_id: UUID string
    /// - index: Integer (chunk position in memory)
    /// - content: String (chunk text)
    /// - size: Integer (char count)
    /// - break_type: String (paragraph|sentence|word|character)
    /// - completeness, coherence, relevance, specificity, metadata: Float (0-1)
    /// - coherence_score: Float (weighted final score)
    /// - quality_level: String (EXCELLENT|GOOD|FAIR|POOR)
    /// - is_code_like: Boolean
    pub fn create_chunk_query() -> &'static str {
        "MERGE (c:MemoryChunk {id: $chunk_id})
        ON CREATE SET
          c.memory_id = $memory_id,
          c.index = $chunk_index,
          c.content = $chunk_content,
          c.size = $chunk_size,
          c.break_type = $break_type,
          c.completeness = $completeness,
          c.coherence = $coherence,
          c.relevance = $relevance,
          c.specificity = $specificity,
          c.metadata = $metadata,
          c.coherence_score = $coherence_score,
          c.quality_level = $quality_level,
          c.is_code_like = $is_code_like,
          c.created_at = timestamp()"
    }

    /// Link a Memory to its MemoryChunk
    pub fn create_contains_relationship_query() -> &'static str {
        "MATCH (m:Memory {id: $memory_id})
        MATCH (c:MemoryChunk {id: $chunk_id})
        MERGE (m)-[r:CONTAINS {index: $chunk_index}]->(c)"
    }

    /// Get all chunks for a memory (ordered by index)
    pub fn get_chunks_for_memory_query() -> &'static str {
        "MATCH (m:Memory {id: $memory_id})-[rel:CONTAINS]->(c:MemoryChunk)
        RETURN c
        ORDER BY rel.index ASC"
    }

    /// Count total MemoryChunk nodes
    pub fn count_chunks_query() -> &'static str {
        "MATCH (c:MemoryChunk) RETURN COUNT(*) as count"
    }

    /// Count CONTAINS relationships
    pub fn count_relationships_query() -> &'static str {
        "MATCH ()-[r:CONTAINS]->() RETURN COUNT(*) as count"
    }

    /// Count memories that have chunks
    pub fn count_memories_with_chunks_query() -> &'static str {
        "MATCH (m:Memory)-[r:CONTAINS]->() RETURN COUNT(DISTINCT m) as count"
    }

    /// Get quality distribution of chunks
    pub fn get_quality_distribution_query() -> &'static str {
        "MATCH (c:MemoryChunk)
        RETURN c.quality_level as level, COUNT(*) as count
        GROUP BY c.quality_level
        ORDER BY count DESC"
    }

    /// Get coherence statistics
    pub fn get_coherence_stats_query() -> &'static str {
        "MATCH (c:MemoryChunk)
        RETURN
          MIN(c.coherence_score) as min_score,
          AVG(c.coherence_score) as avg_score,
          MAX(c.coherence_score) as max_score
        LIMIT 1"
    }

    /// Return all schema creation queries in order
    pub fn all_schema_queries() -> Vec<&'static str> {
        vec![
            Self::create_chunk_id_constraint(),
            Self::create_memory_id_index(),
            Self::create_created_at_index(),
            Self::create_memory_id_index_on_memory(),
        ]
    }
}

/// Statistics about the Neo4j schema
#[derive(Debug, Clone)]
pub struct SchemaStats {
    pub chunk_count: usize,
    pub rel_count: usize,
    pub memory_count: usize,
    pub avg_chunks_per_memory: f64,
    pub quality_distribution: HashMap<String, usize>,
    pub coherence: CoherenceStats,
}

impl SchemaStats {
    /// Create new stats with default values
    pub fn new() -> Self {
        Self {
            chunk_count: 0,
            rel_count: 0,
            memory_count: 0,
            avg_chunks_per_memory: 0.0,
            quality_distribution: HashMap::new(),
            coherence: CoherenceStats::default(),
        }
    }

    /// Check if schema is initialized (has chunks)
    pub fn is_initialized(&self) -> bool {
        self.chunk_count > 0
    }
}

impl Default for SchemaStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Coherence statistics
#[derive(Debug, Clone)]
pub struct CoherenceStats {
    pub min_score: f64,
    pub avg_score: f64,
    pub max_score: f64,
}

impl CoherenceStats {
    /// Create new stats with default values
    pub fn new() -> Self {
        Self {
            min_score: 0.0,
            avg_score: 0.0,
            max_score: 0.0,
        }
    }
}

impl Default for CoherenceStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_id_constraint_query() {
        let query = MemoryChunkSchema::create_chunk_id_constraint();
        assert!(query.contains("CREATE CONSTRAINT"));
        assert!(query.contains("MemoryChunk"));
        assert!(query.contains("UNIQUE"));
    }

    #[test]
    fn test_all_schema_queries() {
        let queries = MemoryChunkSchema::all_schema_queries();
        assert_eq!(queries.len(), 4);
        assert!(queries[0].contains("CONSTRAINT"));
        assert!(queries[1].contains("INDEX"));
    }

    #[test]
    fn test_schema_stats() {
        let stats = SchemaStats::new();
        assert_eq!(stats.chunk_count, 0);
        assert!(!stats.is_initialized());
    }

    #[test]
    fn test_schema_stats_initialized() {
        let mut stats = SchemaStats::new();
        stats.chunk_count = 100;
        assert!(stats.is_initialized());
    }

    #[test]
    fn test_coherence_stats() {
        let stats = CoherenceStats::new();
        assert_eq!(stats.min_score, 0.0);
        assert_eq!(stats.avg_score, 0.0);
        assert_eq!(stats.max_score, 0.0);
    }
}
