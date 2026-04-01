use sqlx::sqlite::SqlitePoolOptions;
use voidm_sqlite::SqliteDatabase;
use voidm_db::Database;

#[tokio::test]
async fn sqlite_title_bm25_finds_title_matches() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    let db = SqliteDatabase::new(pool);
    db.ensure_schema().await.expect("schema");

    sqlx::query("INSERT INTO memories (id, type, title, content, importance, created_at, updated_at) VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem1")
        .bind("semantic")
        .bind("Database Optimization")
        .bind("content unrelated")
        .bind(5i64)
        .execute(&db.pool)
        .await
        .expect("insert memory 1");

    sqlx::query("INSERT INTO memories (id, type, title, content, importance, created_at, updated_at) VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'))")
        .bind("mem2")
        .bind("semantic")
        .bind("Rust Tips")
        .bind("database appears only in content")
        .bind(5i64)
        .execute(&db.pool)
        .await
        .expect("insert memory 2");

    sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES (?, ?, ?)")
        .bind("mem1")
        .bind("Database Optimization")
        .bind("content unrelated")
        .execute(&db.pool)
        .await
        .expect("fts 1");

    sqlx::query("INSERT INTO memories_fts (id, title, content) VALUES (?, ?, ?)")
        .bind("mem2")
        .bind("Rust Tips")
        .bind("database appears only in content")
        .execute(&db.pool)
        .await
        .expect("fts 2");

    let results = db.search_title_bm25("database", None, None, 10).await.expect("search");

    assert!(!results.is_empty());
    assert_eq!(results[0].0, "mem1");
    assert!(results.iter().all(|(id, _)| id != "mem2"));
}
