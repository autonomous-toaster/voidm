/// Neo4j Connection and Database Management
///
/// Handles:
/// - Connection pooling
/// - Schema creation
/// - Transaction management
/// - Error handling

use anyhow::{Context, Result};
use neo4rs::Graph;
use std::sync::Arc;
use tracing::{info, debug, warn};

/// Neo4j database connection wrapper
pub struct Neo4jDb {
    graph: Arc<Graph>,
}

impl Neo4jDb {
    /// Create a new Neo4j database connection
    pub async fn connect(uri: &str, username: &str, password: &str) -> Result<Self> {
        info!("Connecting to Neo4j: {}", uri);

        let graph = Arc::new(
            Graph::new(uri, username, password)
                .await
                .context("Failed to connect to Neo4j")?
        );

        info!("✅ Connected to Neo4j");

        Ok(Neo4jDb { graph })
    }

    /// Create schema (constraints and indexes)
    pub async fn create_schema(&self) -> Result<()> {
        info!("Creating Neo4j schema...");

        // Query 1: UNIQUE constraint on MemoryChunk.id
        debug!("Creating UNIQUE constraint on MemoryChunk.id");
        match self.graph
            .run(
                neo4rs::query("CREATE CONSTRAINT unique_chunk_id IF NOT EXISTS FOR (c:MemoryChunk) REQUIRE c.id IS UNIQUE")
            )
            .await
        {
            Ok(_) => info!("✅ UNIQUE constraint created on MemoryChunk.id"),
            Err(e) if e.to_string().contains("already exists") => {
                debug!("UNIQUE constraint already exists");
            }
            Err(e) => {
                warn!("Failed to create UNIQUE constraint: {}", e);
            }
        }

        // Query 2: Index on MemoryChunk.memory_id
        debug!("Creating index on MemoryChunk.memory_id");
        match self.graph
            .run(
                neo4rs::query("CREATE INDEX idx_chunk_memory_id IF NOT EXISTS FOR (c:MemoryChunk) ON (c.memory_id)")
            )
            .await
        {
            Ok(_) => info!("✅ Index created on MemoryChunk.memory_id"),
            Err(e) if e.to_string().contains("already exists") => {
                debug!("Index already exists");
            }
            Err(e) => {
                warn!("Failed to create index: {}", e);
            }
        }

        // Query 3: Index on MemoryChunk.created_at
        debug!("Creating index on MemoryChunk.created_at");
        match self.graph
            .run(
                neo4rs::query("CREATE INDEX idx_chunk_created_at IF NOT EXISTS FOR (c:MemoryChunk) ON (c.created_at)")
            )
            .await
        {
            Ok(_) => info!("✅ Index created on MemoryChunk.created_at"),
            Err(e) if e.to_string().contains("already exists") => {
                debug!("Index already exists");
            }
            Err(e) => {
                warn!("Failed to create index: {}", e);
            }
        }

        // Query 4: Index on Memory.id
        debug!("Creating index on Memory.id");
        match self.graph
            .run(
                neo4rs::query("CREATE INDEX idx_memory_id IF NOT EXISTS FOR (m:Memory) ON (m.id)")
            )
            .await
        {
            Ok(_) => info!("✅ Index created on Memory.id"),
            Err(e) if e.to_string().contains("already exists") => {
                debug!("Index already exists");
            }
            Err(e) => {
                warn!("Failed to create index: {}", e);
            }
        }

        info!("Schema creation complete");
        Ok(())
    }

    /// Create a MemoryChunk node
    pub async fn create_chunk(
        &self,
        chunk_id: &str,
        memory_id: &str,
        index: i32,
        content: &str,
        size: i32,
        break_type: &str,
        completeness: f32,
        coherence: f32,
        relevance: f32,
        specificity: f32,
        metadata: f32,
        coherence_score: f32,
        quality_level: &str,
        is_code_like: bool,
    ) -> Result<()> {
        let mut txn = self.graph.start_txn().await?;

        txn.run(
            neo4rs::query("CREATE (c:MemoryChunk {id: $id, memory_id: $memory_id, index: $index, content: $content, size: $size, break_type: $break_type, completeness: $completeness, coherence: $coherence, relevance: $relevance, specificity: $specificity, metadata: $metadata, coherence_score: $coherence_score, quality_level: $quality_level, is_code_like: $is_code_like, created_at: datetime()})")
                .param("id", chunk_id)
                .param("memory_id", memory_id)
                .param("index", index)
                .param("content", content)
                .param("size", size)
                .param("break_type", break_type)
                .param("completeness", completeness as f64)
                .param("coherence", coherence as f64)
                .param("relevance", relevance as f64)
                .param("specificity", specificity as f64)
                .param("metadata", metadata as f64)
                .param("coherence_score", coherence_score as f64)
                .param("quality_level", quality_level)
                .param("is_code_like", is_code_like)
        )
        .await?;

        txn.commit().await?;
        Ok(())
    }

    /// Create CONTAINS relationship between Memory and MemoryChunk
    pub async fn create_contains_relationship(
        &self,
        memory_id: &str,
        chunk_id: &str,
        index: i32,
    ) -> Result<()> {
        let query = neo4rs::query(
            "MATCH (m:Memory {id: $memory_id})
             MATCH (c:MemoryChunk {id: $chunk_id})
             CREATE (m)-[r:CONTAINS {index: $index}]->(c)
             RETURN r"
        )
        .param("memory_id", memory_id)
        .param("chunk_id", chunk_id)
        .param("index", index);

        self.graph.run(query).await?;
        Ok(())
    }

    /// Get memory chunk count
    pub async fn get_chunk_count(&self) -> Result<i64> {
        let query = neo4rs::query("MATCH (c:MemoryChunk) RETURN count(c) as count");
        
        let mut result = self.graph.execute(query).await?;
        let row = result.next().await?
            .context("No results from count query")?;
        
        let count: i64 = row.get("count")?;
        Ok(count)
    }

    /// Get average coherence of all chunks
    pub async fn get_average_coherence(&self) -> Result<f64> {
        let query = neo4rs::query(
            "MATCH (c:MemoryChunk) RETURN avg(c.coherence_score) as avg_coherence"
        );
        
        let mut result = self.graph.execute(query).await?;
        let row = result.next().await?
            .context("No results from avg coherence query")?;
        
        let avg: f64 = row.get("avg_coherence")?;
        Ok(avg)
    }

    /// Get chunk count by quality level
    pub async fn get_quality_distribution(&self) -> Result<std::collections::HashMap<String, i64>> {
        let query = neo4rs::query(
            "MATCH (c:MemoryChunk) 
             RETURN c.quality_level as level, count(c) as count
             ORDER BY level"
        );
        
        let mut result = self.graph.execute(query).await?;
        let mut distribution = std::collections::HashMap::new();
        
        while let Some(row) = result.next().await? {
            let level: String = row.get("level")?;
            let count: i64 = row.get("count")?;
            distribution.insert(level, count);
        }
        
        Ok(distribution)
    }

    /// Close connection
    pub async fn close(self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Neo4j running
    async fn test_neo4j_connection() -> Result<()> {
        let db = Neo4jDb::connect("bolt://localhost:7687", "neo4j", "neo4jpassword").await?;
        // Successfully connected
        db.close().await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_schema_creation() -> Result<()> {
        let db = Neo4jDb::connect("bolt://localhost:7687", "neo4j", "neo4jpassword").await?;
        db.create_schema().await?;
        db.close().await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_chunk_creation() -> Result<()> {
        let db = Neo4jDb::connect("bolt://localhost:7687", "neo4j", "neo4jpassword").await?;
        db.create_chunk(
            "mchk_test123",
            "mem-id-123",
            0,
            "Test chunk content",
            18,
            "paragraph",
            0.9,
            0.85,
            0.88,
            0.90,
            0.92,
            0.89,
            "GOOD",
            false,
        ).await?;
        db.close().await?;
        Ok(())
    }
}
