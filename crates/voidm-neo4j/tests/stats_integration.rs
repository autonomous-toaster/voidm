use anyhow::{Context, Result};
use std::env;
use voidm_db::Database;
use voidm_neo4j::Neo4jDatabase;

fn neo4j_test_config() -> Option<(String, String, String, String)> {
    let password = env::var("VOIDM_NEO4J_PASSWORD").ok()?;
    let uri = env::var("VOIDM_NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
    let username = env::var("VOIDM_NEO4J_USERNAME").unwrap_or_else(|_| "neo4j".to_string());
    let database = env::var("VOIDM_NEO4J_DATABASE").unwrap_or_else(|_| "neo4j".to_string());
    Some((uri, username, password, database))
}

async fn connect_test_db() -> Result<Option<Neo4jDatabase>> {
    let Some((uri, username, password, database)) = neo4j_test_config() else {
        eprintln!("skipping Neo4j integration test: VOIDM_NEO4J_PASSWORD not set");
        return Ok(None);
    };

    let db = Neo4jDatabase::connect(&uri, &username, &password, &database)
        .await
        .with_context(|| "failed to connect to Neo4j test database")?;
    Ok(Some(db))
}

#[tokio::test]
async fn neo4j_stats_are_backend_usable() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };

    let stats = db.get_statistics().await?;
    assert!(stats.total_memories >= 0);
    assert!(stats.graph.node_count >= 0);
    assert!(stats.graph.edge_count >= 0);

    let graph = db.get_graph_stats().await?;
    assert!(graph.node_count >= 0);
    assert!(graph.edge_count >= 0);

    Ok(())
}
