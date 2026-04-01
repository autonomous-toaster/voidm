use sqlx::sqlite::SqlitePoolOptions;
use voidm_db::Database;
use voidm_sqlite::SqliteDatabase;

#[tokio::test]
async fn sqlite_generic_graph_supports_entities_and_mentions() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    db.create_node("mchk_ent_1", "MemoryChunk", serde_json::json!({"content": "Alice manages project Apollo"}))
        .await
        .expect("chunk node");

    let (entity_id, created) = db
        .get_or_create_entity("Alice", "person")
        .await
        .expect("create entity");
    assert!(created);

    let (same_entity_id, created_again) = db
        .get_or_create_entity("Alice", "person")
        .await
        .expect("get same entity");
    assert_eq!(entity_id, same_entity_id);
    assert!(!created_again);

    db.link_chunk_to_entity("mchk_ent_1", &entity_id, 0.93)
        .await
        .expect("link mention");

    let entities = db.list_entities().await.expect("list entities");
    assert!(entities.iter().any(|entity| {
        entity["id"] == entity_id && entity["name"] == "Alice" && entity["type"] == "person"
    }));

    let mention_edges = db
        .list_entity_mention_edges()
        .await
        .expect("list mention edges");
    assert!(mention_edges.iter().any(|edge| {
        edge["from"] == "mchk_ent_1"
            && edge["to"] == entity_id
            && edge["type"] == "MENTIONS"
            && edge["confidence"].as_f64().map(|v| (v - 0.93).abs() < 1e-6).unwrap_or(false)
    }));

    assert_eq!(db.count_nodes("Entity").await.expect("entity count"), 1);
    assert_eq!(db.count_edges(Some("MENTIONS")).await.expect("mention count"), 1);
}
