use anyhow::Result;
use sqlx::SqlitePool;
use voidm_sqlite::migrate;

async fn setup_pool() -> SqlitePool {
    SqlitePool::connect("sqlite::memory:").await.expect("pool")
}

#[tokio::test]
async fn migration_preserves_expected_canonical_counts_for_mixed_legacy_db() -> Result<()> {
    let pool = setup_pool().await;

    for stmt in [
        "CREATE TABLE memories (id TEXT PRIMARY KEY, type TEXT NOT NULL, content TEXT NOT NULL, importance INTEGER NOT NULL DEFAULT 5, tags TEXT NOT NULL DEFAULT '[]', metadata TEXT NOT NULL DEFAULT '{}', quality_score REAL, context TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE memory_chunks (id TEXT PRIMARY KEY, memory_id TEXT NOT NULL, text TEXT NOT NULL, \"index\" INTEGER NOT NULL, size INTEGER NOT NULL, break_type TEXT NOT NULL, created_at TEXT NOT NULL, embedding BLOB, embedding_dim INTEGER)",
        "CREATE TABLE chunk_memory_edges (chunk_id TEXT NOT NULL, memory_id TEXT NOT NULL, PRIMARY KEY (chunk_id, memory_id))",
        "CREATE TABLE graph_nodes (id INTEGER PRIMARY KEY AUTOINCREMENT, memory_id TEXT UNIQUE NOT NULL)",
        "CREATE TABLE graph_edges (id INTEGER PRIMARY KEY AUTOINCREMENT, source_id INTEGER NOT NULL, target_id INTEGER NOT NULL, rel_type TEXT NOT NULL, note TEXT, created_at TEXT NOT NULL)",
        "CREATE UNIQUE INDEX idx_graph_edges_unique ON graph_edges(source_id, target_id, rel_type)",
        "CREATE TABLE graph_node_labels (node_id INTEGER NOT NULL, label TEXT NOT NULL, PRIMARY KEY (node_id, label))",
        "CREATE TABLE graph_property_keys (id INTEGER PRIMARY KEY AUTOINCREMENT, key TEXT UNIQUE NOT NULL)",
        "CREATE TABLE graph_node_props_text (node_id INTEGER NOT NULL, key_id INTEGER NOT NULL, value TEXT NOT NULL, PRIMARY KEY (node_id, key_id))",
        "CREATE TABLE graph_edge_props_text (edge_id INTEGER NOT NULL, key_id INTEGER NOT NULL, value TEXT NOT NULL, PRIMARY KEY (edge_id, key_id))"
    ] {
        sqlx::query(stmt).execute(&pool).await?;
    }

    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, created_at, updated_at)
         VALUES
         ('mem_a', 'semantic', 'alpha body', 5, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('mem_b', 'procedural', 'beta body', 5, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('mchk_a1', 'semantic', 'chunk carrier a1', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__memorytype__:semantic', 'semantic', 'type carrier semantic', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__memorytype__:procedural', 'semantic', 'type carrier procedural', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__tag__:ops', 'semantic', 'tag carrier ops', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__scope__:work/ops', 'semantic', 'scope carrier work/ops', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__entity__:alice', 'semantic', 'entity carrier alice', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at)
         VALUES ('mchk_a1', 'mem_a', 'alpha chunk', 0, 11, 'paragraph', '2026-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO chunk_memory_edges (chunk_id, memory_id) VALUES ('mchk_a1', 'mem_a')")
        .execute(&pool)
        .await?;

    sqlx::query(
        "INSERT INTO graph_nodes (memory_id)
         VALUES ('mem_a'), ('mem_b'), ('mchk_a1'), ('__memorytype__:semantic'), ('__memorytype__:procedural'), ('__tag__:ops'), ('__scope__:work/ops'), ('__entity__:alice')"
    )
    .execute(&pool)
    .await?;

    let get_id = |memory_id: &str| {
        let pool = pool.clone();
        let memory_id = memory_id.to_string();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT id FROM graph_nodes WHERE memory_id = ?")
                .bind(memory_id)
                .fetch_one(&pool)
                .await
        }
    };

    let mem_a = get_id("mem_a").await?;
    let mem_b = get_id("mem_b").await?;
    let chunk_a1 = get_id("mchk_a1").await?;
    let mt_sem = get_id("__memorytype__:semantic").await?;
    let mt_proc = get_id("__memorytype__:procedural").await?;
    let tag_ops = get_id("__tag__:ops").await?;
    let scope_ops = get_id("__scope__:work/ops").await?;
    let ent_alice = get_id("__entity__:alice").await?;

    sqlx::query(
        "INSERT INTO graph_node_labels (node_id, label)
         VALUES (?, 'Memory'), (?, 'Memory'), (?, 'MemoryChunk'), (?, 'MemoryType'), (?, 'MemoryType'), (?, 'Tag'), (?, 'Scope'), (?, 'Entity')"
    )
    .bind(mem_a)
    .bind(mem_b)
    .bind(chunk_a1)
    .bind(mt_sem)
    .bind(mt_proc)
    .bind(tag_ops)
    .bind(scope_ops)
    .bind(ent_alice)
    .execute(&pool)
    .await?;

    sqlx::query("INSERT INTO graph_property_keys (key) VALUES ('name')")
        .execute(&pool)
        .await?;
    let name_key: i64 = sqlx::query_scalar("SELECT id FROM graph_property_keys WHERE key = 'name'")
        .fetch_one(&pool)
        .await?;

    sqlx::query(
        "INSERT INTO graph_node_props_text (node_id, key_id, value)
         VALUES (?, ?, 'semantic'), (?, ?, 'procedural'), (?, ?, 'ops'), (?, ?, 'work/ops'), (?, ?, 'Alice')"
    )
    .bind(mt_sem)
    .bind(name_key)
    .bind(mt_proc)
    .bind(name_key)
    .bind(tag_ops)
    .bind(name_key)
    .bind(scope_ops)
    .bind(name_key)
    .bind(ent_alice)
    .bind(name_key)
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO graph_edges (source_id, target_id, rel_type, note, created_at)
         VALUES (?, ?, 'BELONGS_TO', NULL, '2026-01-01T00:00:00Z'),
                (?, ?, 'HAS_TYPE', NULL, '2026-01-01T00:00:00Z'),
                (?, ?, 'HAS_TYPE', NULL, '2026-01-01T00:00:00Z'),
                (?, ?, 'HAS_TAG', NULL, '2026-01-01T00:00:00Z'),
                (?, ?, 'HAS_SCOPE', NULL, '2026-01-01T00:00:00Z'),
                (?, ?, 'MENTIONS', NULL, '2026-01-01T00:00:00Z')"
    )
    .bind(chunk_a1)
    .bind(mem_a)
    .bind(mem_a)
    .bind(mt_sem)
    .bind(mem_b)
    .bind(mt_proc)
    .bind(mem_a)
    .bind(tag_ops)
    .bind(mem_a)
    .bind(scope_ops)
    .bind(chunk_a1)
    .bind(ent_alice)
    .execute(&pool)
    .await?;

    migrate::run(&pool).await?;

    let memory_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'Memory' AND id IN ('mem_a', 'mem_b')")
        .fetch_one(&pool)
        .await?;
    let chunk_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'MemoryChunk' AND id = 'mchk_a1'")
        .fetch_one(&pool)
        .await?;
    let type_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'MemoryType' AND id IN ('__memorytype__:semantic', '__memorytype__:procedural')")
        .fetch_one(&pool)
        .await?;
    let tag_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'Tag' AND id = '__tag__:ops'")
        .fetch_one(&pool)
        .await?;
    let scope_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'Scope' AND id = '__scope__:work/ops'")
        .fetch_one(&pool)
        .await?;
    let entity_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'Entity' AND id = '__entity__:alice'")
        .fetch_one(&pool)
        .await?;

    assert_eq!(memory_count, 2);
    assert_eq!(chunk_count, 1);
    assert_eq!(type_count, 2);
    assert_eq!(tag_count, 1);
    assert_eq!(scope_count, 1);
    assert_eq!(entity_count, 1);

    let has_chunk: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE edge_type = 'HAS_CHUNK' AND from_id = 'mem_a' AND to_id = 'mchk_a1'")
        .fetch_one(&pool)
        .await?;
    let has_type: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE edge_type = 'HAS_TYPE'")
        .fetch_one(&pool)
        .await?;
    let has_tag: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE edge_type = 'HAS_TAG' AND from_id = 'mem_a' AND to_id = '__tag__:ops'")
        .fetch_one(&pool)
        .await?;
    let has_scope: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE edge_type = 'HAS_SCOPE' AND from_id = 'mem_a' AND to_id = '__scope__:work/ops'")
        .fetch_one(&pool)
        .await?;
    let mentions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE edge_type = 'MENTIONS' AND from_id = 'mchk_a1' AND to_id = '__entity__:alice'")
        .fetch_one(&pool)
        .await?;

    assert_eq!(has_chunk, 1);
    assert_eq!(has_type, 2);
    assert_eq!(has_tag, 1);
    assert_eq!(has_scope, 1);
    assert_eq!(mentions, 1);

    let scope_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_scopes WHERE memory_id = 'mem_a' AND scope = 'work/ops'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(scope_rows, 1);

    Ok(())
}
