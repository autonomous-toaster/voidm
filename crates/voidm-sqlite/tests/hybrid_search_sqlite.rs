use sqlx::sqlite::SqlitePoolOptions;
use voidm_core::config::SearchConfig;
use voidm_core::search::{search, SearchMode, SearchOptions};
use voidm_db::Database;
use voidm_sqlite::SqliteDatabase;

#[tokio::test]
async fn sqlite_hybrid_search_groups_title_and_chunk_signals_back_to_memory() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    sqlx::query("INSERT INTO memories (id, type, title, content, importance, quality_score, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem_1")
        .bind("semantic")
        .bind("Database tuning")
        .bind("A long memory body that should not be returned as-is when chunk context is available.")
        .bind(5i64)
        .bind(0.9f64)
        .execute(&db.pool)
        .await
        .expect("insert mem1");

    sqlx::query("INSERT INTO memories (id, type, title, content, importance, quality_score, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem_2")
        .bind("semantic")
        .bind("Rust ownership")
        .bind("Rust memory model details")
        .bind(5i64)
        .bind(0.8f64)
        .execute(&db.pool)
        .await
        .expect("insert mem2");

    sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES (?, ?, ?)")
        .bind("mem_1")
        .bind("Database tuning")
        .bind("A long memory body that should not be returned as-is when chunk context is available.")
        .execute(&db.pool)
        .await
        .expect("fts1");

    sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES (?, ?, ?)")
        .bind("mem_2")
        .bind("Rust ownership")
        .bind("Rust memory model details")
        .execute(&db.pool)
        .await
        .expect("fts2");

    sqlx::query("INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at, embedding, embedding_dim) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?)")
        .bind("mchk_1")
        .bind("mem_1")
        .bind("Most relevant chunk for database tuning")
        .bind(0i64)
        .bind(39i64)
        .bind("sentence")
        .bind(vec![0u8; 8])
        .bind(2i64)
        .execute(&db.pool)
        .await
        .expect("chunk1");

    sqlx::query("INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at, embedding, embedding_dim) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?)")
        .bind("mchk_2")
        .bind("mem_1")
        .bind("Second supporting chunk for database tuning")
        .bind(1i64)
        .bind(42i64)
        .bind("sentence")
        .bind(vec![0u8; 8])
        .bind(2i64)
        .execute(&db.pool)
        .await
        .expect("chunk2");

    sqlx::query("INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at, embedding, embedding_dim) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?)")
        .bind("mchk_3")
        .bind("mem_2")
        .bind("Rust chunk")
        .bind(0i64)
        .bind(10i64)
        .bind("sentence")
        .bind(vec![0u8; 8])
        .bind(2i64)
        .execute(&db.pool)
        .await
        .expect("chunk3");

    sqlx::query("INSERT INTO memories (id, type, content, importance, created_at, updated_at) VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("__memorytype__:semantic")
        .bind("semantic")
        .bind("synthetic memory type carrier")
        .bind(1i64)
        .execute(&db.pool)
        .await
        .expect("synthetic type memory");
    db.create_node("mem_1", "Memory", serde_json::json!({})).await.expect("mem1 node");
    db.create_node("mem_2", "Memory", serde_json::json!({})).await.expect("mem2 node");
    db.create_node("__memorytype__:semantic", "MemoryType", serde_json::json!({"name": "semantic"})).await.expect("type node");
    db.create_edge("mem_1", "HAS_TYPE", "__memorytype__:semantic", None).await.expect("mem1 type");
    db.create_edge("mem_2", "HAS_TYPE", "__memorytype__:semantic", None).await.expect("mem2 type");

    let opts = SearchOptions {
        query: "database tuning".to_string(),
        mode: SearchMode::Rrf,
        limit: 5,
        scope_filter: None,
        type_filter: Some("semantic".to_string()),
        tag_filter: None,
        min_score: None,
        min_quality: None,
        include_neighbors: false,
        neighbor_depth: None,
        neighbor_decay: None,
        neighbor_min_score: None,
        neighbor_limit: None,
        edge_types: None,
        intent: None,
    };

    let mut config = SearchConfig::default();
    config.signals.vector = false;
    config.signals.bm25 = true;
    config.signals.fuzzy = false;
    config.graph_retrieval = None;
    config.metadata_ranking = None;

    let response = search(&db, &opts, "disabled", false, 0.0, &config)
        .await
        .expect("search response");

    assert!(!response.results.is_empty());
    assert_eq!(response.results[0].id, "mem_1");
    assert_eq!(response.results[0].title.as_deref(), Some("Database tuning"));
    assert!(response.results[0].content.len() <= voidm_core::memory_policy::RETRIEVAL_TOTAL_CHAR_BUDGET_PER_MEMORY + 2);
}

#[tokio::test]
async fn sqlite_hybrid_search_respects_type_filter_via_memory_type_nodes() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    sqlx::query("INSERT INTO memories (id, type, title, content, importance, quality_score, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem_1")
        .bind("semantic")
        .bind("Semantic database note")
        .bind("semantic database content")
        .bind(5i64)
        .bind(0.9f64)
        .execute(&db.pool)
        .await
        .expect("insert mem1");
    sqlx::query("INSERT INTO memories (id, type, title, content, importance, quality_score, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem_2")
        .bind("procedural")
        .bind("Procedure for backups")
        .bind("database backup steps")
        .bind(5i64)
        .bind(0.8f64)
        .execute(&db.pool)
        .await
        .expect("insert mem2");
    sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES (?, ?, ?), (?, ?, ?)")
        .bind("mem_1").bind("Semantic database note").bind("semantic database content")
        .bind("mem_2").bind("Procedure for backups").bind("database backup steps")
        .execute(&db.pool)
        .await
        .expect("fts");
    sqlx::query("INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at, embedding, embedding_dim) VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?), (?, ?, ?, ?, ?, ?, datetime('now'), ?, ?)")
        .bind("mchk_1").bind("mem_1").bind("semantic database chunk").bind(0i64).bind(23i64).bind("sentence").bind(vec![0u8; 8]).bind(2i64)
        .bind("mchk_2").bind("mem_2").bind("procedural database chunk").bind(0i64).bind(25i64).bind("sentence").bind(vec![0u8; 8]).bind(2i64)
        .execute(&db.pool)
        .await
        .expect("chunks");

    sqlx::query("INSERT INTO memories (id, type, content, importance, created_at, updated_at) VALUES (?, ?, ?, ?, datetime('now'), datetime('now')), (?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("__memorytype__:semantic").bind("semantic").bind("synthetic semantic type carrier").bind(1i64)
        .bind("__memorytype__:procedural").bind("semantic").bind("synthetic procedural type carrier").bind(1i64)
        .execute(&db.pool)
        .await
        .expect("synthetic type memories");
    db.create_node("mem_1", "Memory", serde_json::json!({})).await.expect("mem1 node");
    db.create_node("mem_2", "Memory", serde_json::json!({})).await.expect("mem2 node");
    db.create_node("__memorytype__:semantic", "MemoryType", serde_json::json!({"name": "semantic"})).await.expect("semantic node");
    db.create_node("__memorytype__:procedural", "MemoryType", serde_json::json!({"name": "procedural"})).await.expect("procedural node");
    db.create_edge("mem_1", "HAS_TYPE", "__memorytype__:semantic", None).await.expect("mem1 type");
    db.create_edge("mem_2", "HAS_TYPE", "__memorytype__:procedural", None).await.expect("mem2 type");

    let opts = SearchOptions {
        query: "database".to_string(),
        mode: SearchMode::Rrf,
        limit: 5,
        scope_filter: None,
        type_filter: Some("semantic".to_string()),
        tag_filter: None,
        min_score: None,
        min_quality: None,
        include_neighbors: false,
        neighbor_depth: None,
        neighbor_decay: None,
        neighbor_min_score: None,
        neighbor_limit: None,
        edge_types: None,
        intent: Some("semantic".to_string()),
    };

    let mut config = SearchConfig::default();
    config.signals.vector = false;
    config.signals.bm25 = true;
    config.signals.fuzzy = false;
    config.graph_retrieval = None;
    config.metadata_ranking = None;

    let response = search(&db, &opts, "disabled", false, 0.0, &config).await.expect("search response");
    assert!(!response.results.is_empty());
    assert!(response.results.iter().all(|r| r.memory_type == "semantic"));
    assert_eq!(response.results[0].id, "mem_1");
}

#[tokio::test]
async fn sqlite_title_search_prefers_title_match_over_content_only_match() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    sqlx::query("INSERT INTO memories (id, type, title, content, importance, created_at, updated_at) VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem_1")
        .bind("semantic")
        .bind("Database tuning")
        .bind("irrelevant body")
        .bind(5i64)
        .execute(&db.pool)
        .await
        .expect("insert mem1");

    sqlx::query("INSERT INTO memories (id, type, title, content, importance, created_at, updated_at) VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem_2")
        .bind("semantic")
        .bind("Other topic")
        .bind("database tuning appears only in content")
        .bind(5i64)
        .execute(&db.pool)
        .await
        .expect("insert mem2");

    sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES (?, ?, ?)")
        .bind("mem_1")
        .bind("Database tuning")
        .bind("irrelevant body")
        .execute(&db.pool)
        .await
        .expect("fts1");

    sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES (?, ?, ?)")
        .bind("mem_2")
        .bind("Other topic")
        .bind("database tuning appears only in content")
        .execute(&db.pool)
        .await
        .expect("fts2");

    sqlx::query("INSERT INTO nodes (id, type, properties, created_at, updated_at) VALUES (?, ?, ?, datetime('now'), datetime('now')), (?, ?, ?, datetime('now'), datetime('now')), (?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem_1").bind("Memory").bind("{}")
        .bind("mem_2").bind("Memory").bind("{}")
        .bind("__memorytype__:semantic").bind("MemoryType").bind("{\"name\":\"semantic\"}")
        .execute(&db.pool)
        .await
        .expect("generic nodes");
    db.create_edge("mem_1", "HAS_TYPE", "__memorytype__:semantic", None).await.expect("mem1 has type");
    db.create_edge("mem_2", "HAS_TYPE", "__memorytype__:semantic", None).await.expect("mem2 has type");

    let title_results = db.search_title_bm25("database", None, Some("semantic"), 10).await.expect("title search");
    assert!(!title_results.is_empty());
    assert_eq!(title_results[0].0, "mem_1");
    assert!(title_results.iter().all(|(id, _)| id != "mem_2"));
}
