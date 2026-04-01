use sqlx::sqlite::SqlitePoolOptions;
use voidm_db::Database;
use voidm_sqlite::SqliteDatabase;

#[tokio::test]
async fn sqlite_jsonl_roundtrip_reconstructs_memory_type_nodes() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    let records = vec![
        serde_json::json!({
            "type": "memory",
            "id": "mem_jsonl_1",
            "content": "procedural memory content",
            "memory_type": "procedural",
            "created_at": "2026-03-31T00:00:00Z",
            "updated_at": "2026-03-31T00:00:00Z",
            "title": "Procedure note",
            "scopes": ["work/ops"],
            "tags": ["ops"],
            "metadata": {"source": "jsonl"},
            "importance": 5,
            "quality_score": 0.8
        })
        .to_string(),
    ];

    let (_memories, _chunks, _rels) = db.import_from_jsonl(records).await.expect("import");

    let memory_rows: Vec<(String, String)> = sqlx::query_as("SELECT id, type FROM memories ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .expect("memory rows");
    assert!(memory_rows.iter().any(|(id, _)| id == "mem_jsonl_1"));

    let exported = db.export_to_jsonl(None).await.expect("export");
    let exported_joined = exported.join("\n");
    assert!(exported_joined.contains("\"memory_type\":\"procedural\""));
    assert!(exported_joined.contains("mem_jsonl_1"));

    let type_edge_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM edges e
         JOIN nodes tn ON tn.id = e.to_id AND tn.type = 'MemoryType'
         WHERE e.from_id = 'mem_jsonl_1'
           AND e.edge_type = 'HAS_TYPE'
           AND json_extract(tn.properties, '$.name') = 'procedural'"
    )
    .fetch_one(&db.pool)
    .await
    .expect("type edge count");

    assert_eq!(type_edge_count, 1);
}

#[tokio::test]
async fn sqlite_jsonl_roundtrip_preserves_chunks_relationships_and_mentions() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    let records = vec![
        serde_json::json!({
            "type": "memory",
            "id": "mem_jsonl_graph_1",
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
            "id": "mem_jsonl_graph_2",
            "content": "This note supports the deployment memory.",
            "memory_type": "semantic",
            "created_at": "2026-03-31T00:00:01Z",
            "updated_at": "2026-03-31T00:00:01Z"
        }).to_string(),
        serde_json::json!({
            "type": "memory_chunk",
            "id": "mchk_jsonl_graph_1",
            "memory_id": "mem_jsonl_graph_1",
            "content": "Alice maintains the deployment playbook.",
            "created_at": "2026-03-31T00:00:00Z"
        }).to_string(),
        serde_json::json!({
            "type": "concept",
            "id": "ent_jsonl_1",
            "name": "Alice",
            "description": "Entity:person",
            "created_at": "2026-03-31T00:00:00Z"
        }).to_string(),
        serde_json::json!({
            "type": "relationship",
            "source_id": "mem_jsonl_graph_2",
            "rel_type": "SUPPORTS",
            "target_id": "mem_jsonl_graph_1",
            "created_at": "2026-03-31T00:00:02Z"
        }).to_string(),
        serde_json::json!({
            "type": "relationship",
            "source_id": "mchk_jsonl_graph_1",
            "rel_type": "MENTIONS",
            "target_id": "ent_jsonl_1",
            "properties": {"confidence": 0.91}
        }).to_string(),
    ];

    let (memories, chunks, rels) = db.import_from_jsonl(records).await.expect("import");
    assert_eq!(memories, 2);
    assert_eq!(chunks, 1);
    assert_eq!(rels, 2);

    let chunk_rows: Vec<(String, String)> = sqlx::query_as("SELECT id, memory_id FROM memory_chunks ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .expect("chunk rows");
    assert!(chunk_rows.iter().any(|(id, memory_id)| id == "mchk_jsonl_graph_1" && memory_id == "mem_jsonl_graph_1"));

    let fetched_chunks = db.fetch_chunks(100).await.expect("fetch chunks");
    assert!(fetched_chunks.iter().any(|(id, _content, memory_id)| id == "mchk_jsonl_graph_1" && memory_id == "mem_jsonl_graph_1"));

    let edges = db.list_edges().await.expect("list edges");
    assert!(edges.iter().any(|edge| {
        edge["from_id"] == "mem_jsonl_graph_2"
            && edge["to_id"] == "mem_jsonl_graph_1"
            && edge["rel_type"] == "SUPPORTS"
    }));

    let entities = db.list_entities().await.expect("entities");
    assert!(entities.iter().any(|entity| entity["name"] == "Alice" && entity["type"] == "person"));

    let mentions = db.list_entity_mention_edges().await.expect("mentions");
    assert!(mentions.iter().any(|edge| {
        edge["from"] == "mchk_jsonl_graph_1"
            && edge["type"] == "MENTIONS"
            && edge["confidence"].as_f64().map(|v| (v - 0.91).abs() < 1e-6).unwrap_or(false)
    }));

    let exported = db.export_to_jsonl(None).await.expect("export").join("\n");
    assert!(exported.contains("mchk_jsonl_graph_1"));
    assert!(exported.contains("\"rel_type\":\"SUPPORTS\""));
    assert!(exported.contains("\"rel_type\":\"MENTIONS\""));
    assert!(exported.contains("Entity:person"));
}
