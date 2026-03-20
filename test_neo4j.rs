use std::sync::Arc;
use voidm_neo4j::Neo4jDatabase;
use voidm_db_trait::Database;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing Neo4j connection...");

    // Connect to local Neo4j instance
    let db = Neo4jDatabase::connect("bolt://localhost:7687", "neo4j", "neo4jneo4j").await?;
    let db = Arc::new(db);

    println!("✅ Connected to Neo4j successfully");

    // Test health check
    match db.health_check().await {
        Ok(_) => println!("✅ Health check passed"),
        Err(e) => println!("❌ Health check failed: {}", e),
    }

    // Test schema initialization
    match db.ensure_schema().await {
        Ok(_) => println!("✅ Schema initialization completed"),
        Err(e) => println!("❌ Schema initialization failed: {}", e),
    }

    // Test adding a memory
    let req_json = serde_json::json!({
        "content": "This is a test memory for Neo4j backend",
        "memory_type": "note",
        "importance": 0.8,
        "tags": ["test", "neo4j"],
        "scopes": ["test"],
        "metadata": {}
    });

    let config_json = serde_json::json!({
        "embeddings": {
            "model": "text-embedding-3-small"
        }
    });

    match db.add_memory(req_json, &config_json).await {
        Ok(response) => {
            println!("✅ Memory added successfully");
            println!("   Response: {}", response);
        }
        Err(e) => println!("❌ Failed to add memory: {}", e),
    }

    // Test listing memories
    match db.list_memories(Some(5)).await {
        Ok(memories) => {
            println!("✅ Listed {} memories", memories.len());
            for (i, memory) in memories.iter().enumerate() {
                println!("   {}: {}", i + 1, memory);
            }
        }
        Err(e) => println!("❌ Failed to list memories: {}", e),
    }

    println!("Neo4j backend test completed!");
    Ok(())
}