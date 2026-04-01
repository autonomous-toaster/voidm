use anyhow::{Context, Result};
use std::env;
use voidm_db::Database;
use voidm_neo4j::Neo4jDatabase;

fn neo4j_test_config() -> Option<(String, String, String, String)> {
    let password = env::var("VOIDM_NEO4J_PASSWORD").ok()?;
    let uri = env::var("VOIDM_NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
    let username = env::var("VOIDM_NEO4J_USERNAME").unwrap_or_else(|_| "neo4j".to_string());
    let database = env::var("VOIDM_NEO4J_DATABASE").unwrap_or_else(|_| "voidmdev".to_string());
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
async fn neo4j_jsonl_roundtrip_reconstructs_memory_type_nodes() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };

    db.query_cypher(
        "MATCH (n) WHERE (n:Memory AND n.id = 'mem_jsonl_neo4j') OR (n:MemoryType AND n.name = 'conceptual') DETACH DELETE n",
        &serde_json::json!({}),
    )
    .await?;

    let records = vec![
        serde_json::json!({
            "type": "memory",
            "id": "mem_jsonl_neo4j",
            "content": "conceptual memory content",
            "memory_type": "conceptual",
            "created_at": "2026-03-31T00:00:00Z",
            "updated_at": "2026-03-31T00:00:00Z",
            "title": "Concept note",
            "scopes": ["work/arch"],
            "metadata": {"source": "jsonl"}
        })
        .to_string(),
    ];

    let (memories, _chunks, _rels) = db.import_from_jsonl(records).await?;
    assert_eq!(memories, 1);

    let exported = db.export_to_jsonl(None).await?;
    let exported_joined = exported.join("\n");
    assert!(exported_joined.contains("\"memory_type\":\"conceptual\""));
    assert!(exported_joined.contains("mem_jsonl_neo4j"));

    let typed_mem = db.get_memory("mem_jsonl_neo4j").await?;
    let typed_mem = typed_mem.expect("typed memory should exist");
    assert_eq!(typed_mem.get("type").and_then(|v| v.as_str()), Some("conceptual"));

    Ok(())
}

#[tokio::test]
async fn neo4j_jsonl_roundtrip_preserves_chunks_relationships_and_mentions() -> Result<()> {
    let Some(db) = connect_test_db().await? else {
        return Ok(());
    };

    db.query_cypher(
        "MATCH (n) WHERE (n:Memory AND n.id IN ['mem_jsonl_graph_n4j_1','mem_jsonl_graph_n4j_2']) OR (n:MemoryChunk AND n.id = 'mchk_jsonl_graph_n4j_1') OR (n:Entity AND n.name = 'Alice') DETACH DELETE n",
        &serde_json::json!({}),
    ).await?;

    let records = vec![
        serde_json::json!({
            "type": "memory",
            "id": "mem_jsonl_graph_n4j_1",
            "content": "Alice maintains the deployment playbook.",
            "memory_type": "semantic",
            "created_at": "2026-03-31T00:00:00Z",
            "updated_at": "2026-03-31T00:00:00Z",
            "title": "Deployment memory",
            "scopes": ["work/ops"],
            "tags": ["ops"],
            "metadata": {"source": "jsonl"}
        }).to_string(),
        serde_json::json!({
            "type": "memory",
            "id": "mem_jsonl_graph_n4j_2",
            "content": "This note supports the deployment memory.",
            "memory_type": "semantic",
            "created_at": "2026-03-31T00:00:01Z",
            "updated_at": "2026-03-31T00:00:01Z"
        }).to_string(),
        serde_json::json!({
            "type": "memory_chunk",
            "id": "mchk_jsonl_graph_n4j_1",
            "memory_id": "mem_jsonl_graph_n4j_1",
            "content": "Alice maintains the deployment playbook.",
            "created_at": "2026-03-31T00:00:00Z"
        }).to_string(),
        serde_json::json!({
            "type": "concept",
            "id": "ent_jsonl_n4j_1",
            "name": "Alice",
            "description": "Entity:person",
            "created_at": "2026-03-31T00:00:00Z"
        }).to_string(),
        serde_json::json!({
            "type": "relationship",
            "source_id": "mem_jsonl_graph_n4j_2",
            "rel_type": "SUPPORTS",
            "target_id": "mem_jsonl_graph_n4j_1",
            "created_at": "2026-03-31T00:00:02Z"
        }).to_string(),
        serde_json::json!({
            "type": "relationship",
            "source_id": "mchk_jsonl_graph_n4j_1",
            "rel_type": "MENTIONS",
            "target_id": "ent_jsonl_n4j_1",
            "properties": {"confidence": 0.91}
        }).to_string(),
    ];

    let (memories, chunks, rels) = db.import_from_jsonl(records).await?;
    assert_eq!(memories, 2);
    assert_eq!(chunks, 1);
    assert_eq!(rels, 2);

    let chunk = db.get_chunk("mchk_jsonl_graph_n4j_1").await?.expect("chunk exists");
    assert_eq!(chunk["memory_id"], "mem_jsonl_graph_n4j_1");

    let mem2 = db.get_memory("mem_jsonl_graph_n4j_2").await?;
    assert!(mem2.is_some(), "second imported memory should exist");

    let entities = db.list_entities().await?;
    assert!(entities.iter().any(|entity| entity["name"] == "Alice" && entity["type"] == "person"));

    let mentions = db.list_entity_mention_edges().await?;
    assert!(mentions.iter().any(|edge| {
        edge["from"] == "mchk_jsonl_graph_n4j_1"
            && edge["to"].is_string()
            && edge["type"] == "MENTIONS"
            && edge["confidence"].as_f64().map(|v| (v - 0.91).abs() < 1e-6).unwrap_or(false)
    }));

    let exported = db.export_to_jsonl(None).await?.join("\n");
    assert!(exported.contains("mchk_jsonl_graph_n4j_1"));
    assert!(exported.contains("\"rel_type\":\"SUPPORTS\""));
    assert!(exported.contains("\"rel_type\":\"MENTIONS\""));
    assert!(exported.contains("Entity:person"));

    Ok(())
}
