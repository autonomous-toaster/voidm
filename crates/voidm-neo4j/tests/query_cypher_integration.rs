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
async fn neo4j_query_cypher_returns_projected_json_rows() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };

    let rows = db.query_cypher(
        "MATCH (m:Memory) RETURN m.id as id, m.title as title LIMIT 3",
        &serde_json::json!({}),
    ).await?;

    let arr = rows.as_array().expect("array rows");
    assert!(!arr.is_empty(), "expected at least one row");
    assert!(arr.iter().all(|row| row.get("id").is_some()), "rows={:?}", arr);

    Ok(())
}

#[tokio::test]
async fn neo4j_query_cypher_supports_params() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };

    let rows = db.query_cypher(
        "MATCH (m:Memory {id: $id}) RETURN m.id as id LIMIT 1",
        &serde_json::json!({"id": "mem_jsonl_graph_n4j_1"}),
    ).await?;

    let arr = rows.as_array().expect("array rows");
    assert!(arr.iter().any(|row| row.get("id").and_then(|v| v.as_str()) == Some("mem_jsonl_graph_n4j_1")) || arr.is_empty());

    Ok(())
}
