use anyhow::Result;
use sqlx::SqlitePool;
use voidm_db::Database;
use voidm_sqlite::SqliteDatabase;

async fn setup_db() -> SqliteDatabase {
    let pool = SqlitePool::connect("sqlite::memory:").await.expect("pool");
    let db = SqliteDatabase::new(pool.clone());
    db.ensure_schema().await.expect("schema");
    db
}

#[tokio::test]
async fn scope_reads_and_filters_use_canonical_has_scope_graph() -> Result<()> {
    let db = setup_db().await;
    let pool = db.pool.clone();

    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, quality_score, context, created_at, updated_at, title)
         VALUES ('mem_scope_can_1', 'semantic', 'body', 5, '[]', '{}', NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'scope title')"
    )
    .execute(&pool)
    .await?;

    sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES ('mem_scope_can_1', 'scope title', 'body')")
        .execute(&pool)
        .await?;

    sqlx::query("INSERT INTO nodes (id, type, properties, created_at, updated_at) VALUES ('mem_scope_can_1', 'Memory', '{\"title\":\"scope title\"}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO nodes (id, type, properties, created_at, updated_at) VALUES ('__scope__:work/app', 'Scope', '{\"name\":\"work/app\"}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO nodes (id, type, properties, created_at, updated_at) VALUES ('__memorytype__:semantic', 'MemoryType', '{\"name\":\"semantic\"}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES ('mem_scope_can_1:HAS_SCOPE:__scope__:work/app', 'mem_scope_can_1', 'HAS_SCOPE', '__scope__:work/app', '{}', '2026-01-01T00:00:00Z')")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO edges (id, from_id, edge_type, to_id, properties, created_at) VALUES ('mem_scope_can_1:HAS_TYPE:__memorytype__:semantic', 'mem_scope_can_1', 'HAS_TYPE', '__memorytype__:semantic', '{}', '2026-01-01T00:00:00Z')")
        .execute(&pool)
        .await?;

    // Deliberately do not populate memory_scopes. Reads must still work.
    let mem = db.get_memory("mem_scope_can_1").await?.expect("memory");
    let scopes = mem["scopes"].as_array().cloned().unwrap_or_default();
    assert!(scopes.iter().any(|s| s.as_str() == Some("work/app")));

    let titled = db.search_title_bm25("scope", Some("work/app"), None, 10).await?;
    assert!(titled.iter().any(|(id, _)| id == "mem_scope_can_1"));

    Ok(())
}
