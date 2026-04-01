use sqlx::sqlite::SqlitePoolOptions;
use voidm_db::Database;
use voidm_sqlite::SqliteDatabase;

#[tokio::test]
async fn sqlite_generic_graph_supports_has_tag_and_has_chunk() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    db.create_node("mem_graph_1", "Memory", serde_json::json!({"title": "Graph memory"}))
        .await
        .expect("memory node");
    db.create_node("tag_ops", "Tag", serde_json::json!({"name": "ops"}))
        .await
        .expect("tag node");
    db.create_node(
        "mchk_graph_1_0",
        "MemoryChunk",
        serde_json::json!({"content": "chunk body", "sequence_num": 0, "memory_id": "mem_graph_1"}),
    )
    .await
    .expect("chunk node");

    db.create_edge("mem_graph_1", "HAS_TAG", "tag_ops", None)
        .await
        .expect("has tag edge");
    db.create_edge(
        "mem_graph_1",
        "HAS_CHUNK",
        "mchk_graph_1_0",
        Some(serde_json::json!({"sequence_num": 0})),
    )
    .await
    .expect("has chunk edge");

    let tag_edges = db.list_tag_edges().await.expect("list tag edges");
    assert!(tag_edges.iter().any(|edge| {
        edge["from"] == "mem_graph_1" && edge["to"] == "tag_ops" && edge["type"] == "HAS_TAG"
    }));

    let chunk_edges = db.list_chunk_edges().await.expect("list chunk edges");
    assert!(chunk_edges.iter().any(|edge| {
        edge["from"] == "mem_graph_1" && edge["to"] == "mchk_graph_1_0" && edge["type"] == "HAS_CHUNK"
    }));

    assert_eq!(db.count_edges(Some("HAS_TAG")).await.expect("count has tag"), 1);
    assert_eq!(db.count_edges(Some("HAS_CHUNK")).await.expect("count has chunk"), 1);
}

#[tokio::test]
async fn sqlite_graph_ops_pagerank_data_uses_generic_memory_nodes_and_edges() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool.clone());
    db.ensure_schema().await.expect("schema");

    db.create_node("mem_a", "Memory", serde_json::json!({}))
        .await
        .expect("mem a");
    db.create_node("mem_b", "Memory", serde_json::json!({}))
        .await
        .expect("mem b");
    db.create_node("tag_x", "Tag", serde_json::json!({}))
        .await
        .expect("tag");

    db.create_edge("mem_a", "RELATED_TO", "mem_b", None)
        .await
        .expect("memory edge");
    db.create_edge("mem_a", "HAS_TAG", "tag_x", None)
        .await
        .expect("tag edge");

    let ops = db.graph_ops();
    let nodes = ops.get_all_memory_nodes().await.expect("memory nodes");
    let edges = ops.get_all_memory_edges().await.expect("memory edges");
    let stats = ops.get_graph_stats().await.expect("graph stats");

    assert_eq!(nodes.len(), 2);
    assert!(nodes.iter().any(|(_, id)| id == "mem_a"));
    assert!(nodes.iter().any(|(_, id)| id == "mem_b"));
    assert_eq!(edges.len(), 1);
    assert!(stats.0 >= 3);
    assert!(stats.1 >= 2);
    assert_eq!(stats.2.get("RELATED_TO").copied(), Some(1));
    assert_eq!(stats.2.get("HAS_TAG").copied(), Some(1));
}
