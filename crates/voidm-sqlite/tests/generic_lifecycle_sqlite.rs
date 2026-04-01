use sqlx::sqlite::SqlitePoolOptions;
use voidm_core::Config;
use voidm_db::Database;
use voidm_sqlite::SqliteDatabase;

#[tokio::test]
async fn sqlite_add_update_delete_keeps_generic_graph_consistent() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    let req = serde_json::json!({
        "content": "This is a semantic memory about tuning PostgreSQL indexes and query planning. It should be long enough to produce at least one canonical chunk in the normal add flow.",
        "memory_type": "semantic",
        "importance": 6,
        "tags": ["database", "postgres"],
        "metadata": {"source": "lifecycle-test"},
        "scopes": [],
        "links": [],
        "title": "Postgres tuning"
    });
    let mut config_obj = Config::default();
    config_obj.embeddings.enabled = false;
    let config = serde_json::to_value(config_obj).expect("config json");

    let add_resp = db.add_memory(req, &config).await.expect("add memory");
    let memory_id = add_resp["id"].as_str().expect("memory id").to_string();

    let memory_node = db.get_node(&memory_id).await.expect("get memory node");
    assert!(memory_node.is_some());
    assert_eq!(memory_node.unwrap()["type"], "Memory");

    let type_edge = db
        .get_edge(&memory_id, "HAS_TYPE", "__memorytype__:semantic")
        .await
        .expect("get type edge");
    assert!(type_edge.is_some());

    let chunk_edges = db
        .get_node_edges(&memory_id, Some("HAS_CHUNK"))
        .await
        .expect("chunk edges after add");
    assert!(!chunk_edges.is_empty());
    let chunk_ids: Vec<String> = chunk_edges
        .iter()
        .filter_map(|edge| edge["to_id"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(!chunk_ids.is_empty());
    for chunk_id in &chunk_ids {
        let node = db.get_node(chunk_id).await.expect("chunk node");
        assert!(node.is_some());
        assert_eq!(node.unwrap()["type"], "MemoryChunk");
    }

    db.update_memory(
        &memory_id,
        "Updated semantic memory content about query planning, VACUUM strategy, and index maintenance. This should regenerate canonical chunks and keep graph edges consistent.",
    )
    .await
    .expect("update memory");

    let updated_chunk_edges = db
        .get_node_edges(&memory_id, Some("HAS_CHUNK"))
        .await
        .expect("chunk edges after update");
    assert!(!updated_chunk_edges.is_empty());
    let updated_chunk_ids: Vec<String> = updated_chunk_edges
        .iter()
        .filter_map(|edge| edge["to_id"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(!updated_chunk_ids.is_empty());
    for chunk_id in &updated_chunk_ids {
        let node = db.get_node(chunk_id).await.expect("updated chunk node");
        assert!(node.is_some());
        assert_eq!(node.unwrap()["type"], "MemoryChunk");
    }

    db.delete_memory(&memory_id).await.expect("delete memory");

    assert!(db.get_node(&memory_id).await.expect("memory node after delete").is_none());
    assert!(db
        .get_edge(&memory_id, "HAS_TYPE", "__memorytype__:semantic")
        .await
        .expect("type edge after delete")
        .is_none());
    assert!(db
        .get_node_edges(&memory_id, Some("HAS_CHUNK"))
        .await
        .expect("chunk edges after delete")
        .is_empty());
    for chunk_id in updated_chunk_ids {
        assert!(db.get_node(&chunk_id).await.expect("deleted chunk node").is_none());
    }
}
