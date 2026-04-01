use sqlx::sqlite::SqlitePoolOptions;
use voidm_core::Config;
use voidm_db::Database;
use voidm_sqlite::SqliteDatabase;

#[tokio::test]
async fn sqlite_add_memory_with_scopes_and_tags_persists_canonical_scope_and_tag_edges() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    let mut config = Config::default();
    config.embeddings.enabled = false;
    let config_json = serde_json::to_value(&config).expect("config json");

    let req = serde_json::json!({
        "content": "Scoped platform operations memory with explicit tags.",
        "memory_type": "semantic",
        "scopes": ["work/platform"],
        "tags": ["docker", "kubernetes"],
        "importance": 5,
        "metadata": {},
        "links": [],
        "context": null,
        "title": "Scoped platform note"
    });

    let response = db.add_memory(req, &config_json).await.expect("add memory");
    let memory_id = response["id"].as_str().expect("memory id").to_string();

    let scope_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edges WHERE from_id = ? AND edge_type = 'HAS_SCOPE'"
    )
    .bind(&memory_id)
    .fetch_one(&db.pool)
    .await
    .expect("scope count");
    assert_eq!(scope_count, 1);

    let tag_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edges WHERE from_id = ? AND edge_type = 'HAS_TAG'"
    )
    .bind(&memory_id)
    .fetch_one(&db.pool)
    .await
    .expect("tag count");
    assert_eq!(tag_count, 2);

    let memory = db.get_memory(&memory_id).await.expect("get memory").expect("memory exists");
    let scopes = memory["scopes"].as_array().cloned().unwrap_or_default();
    assert!(scopes.iter().any(|s| s.as_str() == Some("work/platform")));
}
