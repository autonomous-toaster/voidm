use anyhow::Result;
use sqlx::SqlitePool;
use voidm_sqlite::migrate;

async fn setup_pool() -> SqlitePool {
    SqlitePool::connect("sqlite::memory:").await.expect("pool")
}

#[tokio::test]
async fn migration_sets_legacy_backfill_meta_markers() -> Result<()> {
    let pool = setup_pool().await;
    migrate::run(&pool).await?;

    let canonical: String = sqlx::query_scalar("SELECT value FROM db_meta WHERE key = 'graph_storage_canonical'")
        .fetch_one(&pool)
        .await?;
    let backfill_version: String = sqlx::query_scalar("SELECT value FROM db_meta WHERE key = 'legacy_graph_backfill_version'")
        .fetch_one(&pool)
        .await?;
    let legacy_policy: String = sqlx::query_scalar("SELECT value FROM db_meta WHERE key = 'legacy_graph_policy'")
        .fetch_one(&pool)
        .await?;

    assert_eq!(canonical, "nodes_edges");
    assert_eq!(backfill_version, "1");
    assert_eq!(legacy_policy, "migration_input_only");

    Ok(())
}
