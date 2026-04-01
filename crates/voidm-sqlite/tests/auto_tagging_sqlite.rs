use sqlx::sqlite::SqlitePoolOptions;
use voidm_core::Config;
use voidm_db::Database;
use voidm_sqlite::SqliteDatabase;

#[tokio::test]
async fn sqlite_add_memory_generates_and_persists_strict_auto_tags() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    let mut config = Config::default();
    config.embeddings.enabled = false;
    config.enrichment.auto_tagging.enabled = true;
    config.enrichment.auto_tagging.model = "tinyllama".to_string();
    config.enrichment.auto_tagging.max_tags = 5;
    let config_json = serde_json::to_value(&config).expect("config json");

    let req = serde_json::json!({
        "content": "Docker orchestration for Kubernetes deployment pipelines and container operations.",
        "memory_type": "semantic",
        "scopes": [],
        "tags": [],
        "importance": 5,
        "metadata": {},
        "links": [],
        "context": null,
        "title": "Container platform note"
    });

    let generated = voidm_core::auto_tagging::generate_tags(
        "Docker orchestration for Kubernetes deployment pipelines and container operations.",
        &config,
    )
    .await
    .expect("generate tags");
    assert!(!generated.is_empty(), "expected TinyLLaMA strict generator to return tags");

    let response = db.add_memory(req, &config_json).await.expect("add memory");
    let memory_id = response["id"].as_str().expect("memory id").to_string();
    let tags = response["tags"].as_array().cloned().unwrap_or_default();
    assert!(!tags.is_empty(), "expected generated tags in add response");

    let memory = db.get_memory(&memory_id).await.expect("get memory").expect("memory exists");
    let persisted_tags = memory["tags"].as_array().cloned().unwrap_or_default();
    assert!(!persisted_tags.is_empty(), "expected persisted tags");

    let metadata = memory["metadata"].as_object().cloned().unwrap_or_default();
    let auto_generated = metadata.get("auto_generated_tags").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    assert!(!auto_generated.is_empty(), "expected metadata.auto_generated_tags");

    let has_tag_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edges WHERE from_id = ? AND edge_type = 'HAS_TAG'"
    )
    .bind(&memory_id)
    .fetch_one(&db.pool)
    .await
    .expect("has tag count");
    assert!(has_tag_count >= 1, "expected canonical HAS_TAG edges");
}
