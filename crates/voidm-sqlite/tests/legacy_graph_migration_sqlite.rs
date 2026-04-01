use anyhow::Result;
use sqlx::{Row, SqlitePool};
use voidm_db::Database;
use voidm_sqlite::{migrate, SqliteDatabase};

async fn setup_pool() -> SqlitePool {
    SqlitePool::connect("sqlite::memory:").await.expect("pool")
}

#[tokio::test]
async fn migration_backfills_canonical_graph_from_legacy_tables_without_data_loss() -> Result<()> {
    let pool = setup_pool().await;

    // Simulate an old DB that already has legacy graph data before canonical backfill runs.
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
        "CREATE TABLE graph_edge_props_text (edge_id INTEGER NOT NULL, key_id INTEGER NOT NULL, value TEXT NOT NULL, PRIMARY KEY (edge_id, key_id))",
    ] {
        sqlx::query(stmt).execute(&pool).await?;
    }

    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, created_at, updated_at)
         VALUES
         ('mem_legacy_1', 'semantic', 'legacy memory body', 5, '[\"ops\"]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__memorytype__:semantic', 'semantic', 'synthetic MemoryType carrier', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__tag__:ops', 'semantic', 'synthetic Tag carrier', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at)
         VALUES ('mchk_legacy_1', 'mem_legacy_1', 'legacy chunk text', 0, 17, 'paragraph', '2026-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await?;

    sqlx::query("INSERT INTO chunk_memory_edges (chunk_id, memory_id) VALUES ('mchk_legacy_1', 'mem_legacy_1')")
        .execute(&pool)
        .await?;

    sqlx::query(
        "INSERT INTO graph_nodes (memory_id) VALUES ('mem_legacy_1'), ('__memorytype__:semantic'), ('__tag__:ops')"
    )
    .execute(&pool)
    .await?;

    let mem_node_id: i64 = sqlx::query("SELECT id FROM graph_nodes WHERE memory_id = 'mem_legacy_1'")
        .fetch_one(&pool)
        .await?
        .get(0);
    let type_node_id: i64 = sqlx::query("SELECT id FROM graph_nodes WHERE memory_id = '__memorytype__:semantic'")
        .fetch_one(&pool)
        .await?
        .get(0);
    let tag_node_id: i64 = sqlx::query("SELECT id FROM graph_nodes WHERE memory_id = '__tag__:ops'")
        .fetch_one(&pool)
        .await?
        .get(0);

    sqlx::query(
        "INSERT INTO graph_node_labels (node_id, label) VALUES (?, 'Memory'), (?, 'MemoryType'), (?, 'Tag')"
    )
    .bind(mem_node_id)
    .bind(type_node_id)
    .bind(tag_node_id)
    .execute(&pool)
    .await?;

    sqlx::query("INSERT INTO graph_property_keys (key) VALUES ('name')")
        .execute(&pool)
        .await?;
    let name_key_id: i64 = sqlx::query("SELECT id FROM graph_property_keys WHERE key = 'name'")
        .fetch_one(&pool)
        .await?
        .get(0);

    sqlx::query(
        "INSERT INTO graph_node_props_text (node_id, key_id, value) VALUES (?, ?, 'semantic'), (?, ?, 'ops')"
    )
    .bind(type_node_id)
    .bind(name_key_id)
    .bind(tag_node_id)
    .bind(name_key_id)
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO graph_edges (source_id, target_id, rel_type, note, created_at)
         VALUES (?, ?, 'HAS_TYPE', NULL, '2026-01-01T00:00:00Z'),
                (?, ?, 'HAS_TAG', NULL, '2026-01-01T00:00:00Z')"
    )
    .bind(mem_node_id)
    .bind(type_node_id)
    .bind(mem_node_id)
    .bind(tag_node_id)
    .execute(&pool)
    .await?;

    migrate::run(&pool).await?;
    let db = SqliteDatabase::new(pool.clone());

    let memory_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'Memory' AND id = 'mem_legacy_1'")
        .fetch_one(&pool)
        .await?;
    let chunk_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'MemoryChunk' AND id = 'mchk_legacy_1'")
        .fetch_one(&pool)
        .await?;
    let type_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'MemoryType' AND id = '__memorytype__:semantic'")
        .fetch_one(&pool)
        .await?;
    let tag_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE type = 'Tag' AND id = '__tag__:ops'")
        .fetch_one(&pool)
        .await?;

    assert_eq!(memory_nodes, 1);
    assert_eq!(chunk_nodes, 1);
    if type_nodes != 1 || tag_nodes != 1 {
        let rows = sqlx::query("SELECT id, type, properties FROM nodes ORDER BY id")
            .fetch_all(&pool)
            .await?;
        panic!("unexpected canonical nodes: {:?}", rows.iter().map(|r| (
            r.get::<String, _>(0),
            r.get::<String, _>(1),
            r.get::<String, _>(2)
        )).collect::<Vec<_>>());
    }
    assert_eq!(type_nodes, 1);
    assert_eq!(tag_nodes, 1);

    let has_chunk: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE from_id = 'mem_legacy_1' AND edge_type = 'HAS_CHUNK' AND to_id = 'mchk_legacy_1'")
        .fetch_one(&pool)
        .await?;
    let has_type: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE from_id = 'mem_legacy_1' AND edge_type = 'HAS_TYPE' AND to_id = '__memorytype__:semantic'")
        .fetch_one(&pool)
        .await?;
    let has_tag: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE from_id = 'mem_legacy_1' AND edge_type = 'HAS_TAG' AND to_id = '__tag__:ops'")
        .fetch_one(&pool)
        .await?;

    assert_eq!(has_chunk, 1);
    assert_eq!(has_type, 1);
    assert_eq!(has_tag, 1);

    let has_type_for_read: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE from_id = 'mem_legacy_1' AND edge_type = 'HAS_TYPE' AND to_id = '__memorytype__:semantic'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(has_type_for_read, 1);

    let mem = db.get_memory("mem_legacy_1").await?.expect("memory exists");
    assert_eq!(mem["id"].as_str(), Some("mem_legacy_1"));

    let tag_edges = db.list_tag_edges().await?;
    assert!(tag_edges.iter().any(|e| e["from"] == "mem_legacy_1" && e["to"] == "__tag__:ops"));

    Ok(())
}

#[tokio::test]
async fn migration_backfills_legacy_scope_and_entity_graph() -> Result<()> {
    let pool = setup_pool().await;

    for stmt in [
        "CREATE TABLE memories (id TEXT PRIMARY KEY, type TEXT NOT NULL, content TEXT NOT NULL, importance INTEGER NOT NULL DEFAULT 5, tags TEXT NOT NULL DEFAULT '[]', metadata TEXT NOT NULL DEFAULT '{}', quality_score REAL, context TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
        "CREATE TABLE memory_chunks (id TEXT PRIMARY KEY, memory_id TEXT NOT NULL, text TEXT NOT NULL, \"index\" INTEGER NOT NULL, size INTEGER NOT NULL, break_type TEXT NOT NULL, created_at TEXT NOT NULL, embedding BLOB, embedding_dim INTEGER)",
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
         ('mem_scope_entity_legacy', 'semantic', 'Alice tuned PostgreSQL for ops', 5, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('mchk_scope_entity_legacy', 'semantic', 'synthetic chunk carrier', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__scope__:work/ops', 'semantic', 'synthetic scope carrier', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('__entity__:alice', 'semantic', 'synthetic entity carrier', 1, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at)
         VALUES ('mchk_scope_entity_legacy', 'mem_scope_entity_legacy', 'Alice tuned PostgreSQL for ops', 0, 31, 'paragraph', '2026-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO graph_nodes (memory_id)
         VALUES ('mem_scope_entity_legacy'), ('mchk_scope_entity_legacy'), ('__scope__:work/ops'), ('__entity__:alice')"
    )
    .execute(&pool)
    .await?;

    let mem_node_id: i64 = sqlx::query("SELECT id FROM graph_nodes WHERE memory_id = 'mem_scope_entity_legacy'")
        .fetch_one(&pool)
        .await?
        .get(0);
    let chunk_node_id: i64 = sqlx::query("SELECT id FROM graph_nodes WHERE memory_id = 'mchk_scope_entity_legacy'")
        .fetch_one(&pool)
        .await?
        .get(0);
    let scope_node_id: i64 = sqlx::query("SELECT id FROM graph_nodes WHERE memory_id = '__scope__:work/ops'")
        .fetch_one(&pool)
        .await?
        .get(0);
    let entity_node_id: i64 = sqlx::query("SELECT id FROM graph_nodes WHERE memory_id = '__entity__:alice'")
        .fetch_one(&pool)
        .await?
        .get(0);

    sqlx::query(
        "INSERT INTO graph_node_labels (node_id, label)
         VALUES (?, 'Memory'), (?, 'MemoryChunk'), (?, 'Scope'), (?, 'Entity')"
    )
    .bind(mem_node_id)
    .bind(chunk_node_id)
    .bind(scope_node_id)
    .bind(entity_node_id)
    .execute(&pool)
    .await?;

    sqlx::query("INSERT INTO graph_property_keys (key) VALUES ('name')")
        .execute(&pool)
        .await?;
    let name_key_id: i64 = sqlx::query("SELECT id FROM graph_property_keys WHERE key = 'name'")
        .fetch_one(&pool)
        .await?
        .get(0);

    sqlx::query(
        "INSERT INTO graph_node_props_text (node_id, key_id, value)
         VALUES (?, ?, 'work/ops'), (?, ?, 'Alice')"
    )
    .bind(scope_node_id)
    .bind(name_key_id)
    .bind(entity_node_id)
    .bind(name_key_id)
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO graph_edges (source_id, target_id, rel_type, note, created_at)
         VALUES (?, ?, 'BELONGS_TO', NULL, '2026-01-01T00:00:00Z'),
                (?, ?, 'HAS_SCOPE', NULL, '2026-01-01T00:00:00Z'),
                (?, ?, 'MENTIONS', NULL, '2026-01-01T00:00:00Z')"
    )
    .bind(chunk_node_id)
    .bind(mem_node_id)
    .bind(mem_node_id)
    .bind(scope_node_id)
    .bind(chunk_node_id)
    .bind(entity_node_id)
    .execute(&pool)
    .await?;

    migrate::run(&pool).await?;
    let db = SqliteDatabase::new(pool.clone());

    let scope_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = '__scope__:work/ops' AND type = 'Scope'")
        .fetch_one(&pool)
        .await?;
    let entity_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = '__entity__:alice' AND type = 'Entity'")
        .fetch_one(&pool)
        .await?;
    let has_scope: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE from_id = 'mem_scope_entity_legacy' AND edge_type = 'HAS_SCOPE' AND to_id = '__scope__:work/ops'")
        .fetch_one(&pool)
        .await?;
    let mentions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE from_id = 'mchk_scope_entity_legacy' AND edge_type = 'MENTIONS' AND to_id = '__entity__:alice'")
        .fetch_one(&pool)
        .await?;

    assert_eq!(scope_nodes, 1);
    assert_eq!(entity_nodes, 1);
    assert_eq!(has_scope, 1);
    assert_eq!(mentions, 1);

    let scopes = sqlx::query_scalar::<_, String>("SELECT scope FROM memory_scopes WHERE memory_id = 'mem_scope_entity_legacy'")
        .fetch_all(&pool)
        .await?;
    assert!(scopes.iter().any(|s| s == "work/ops"));

    let mem = db.get_memory("mem_scope_entity_legacy").await?.expect("memory exists");
    let scopes_json = mem["scopes"].as_array().cloned().unwrap_or_default();
    assert!(scopes_json.iter().any(|s| s.as_str() == Some("work/ops")));

    let entities = db.list_entities().await?;
    assert!(entities.iter().any(|e| e["id"] == "__entity__:alice"));

    let mention_edges = db.list_entity_mention_edges().await?;
    assert!(mention_edges.iter().any(|e| e["from"] == "mchk_scope_entity_legacy" && e["to"] == "__entity__:alice"));

    Ok(())
}

#[tokio::test]
async fn migration_backfill_is_idempotent() -> Result<()> {
    let pool = setup_pool().await;
    migrate::run(&pool).await?;

    sqlx::query(
        "INSERT INTO memories (id, type, content, importance, tags, metadata, created_at, updated_at)
         VALUES ('mem_idem_1', 'semantic', 'body', 5, '[]', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory_chunks (id, memory_id, text, \"index\", size, break_type, created_at)
         VALUES ('mchk_idem_1', 'mem_idem_1', 'chunk body', 0, 10, 'paragraph', '2026-01-01T00:00:00Z')"
    )
    .execute(&pool)
    .await?;

    migrate::run(&pool).await?;
    migrate::run(&pool).await?;

    let memory_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = 'mem_idem_1' AND type = 'Memory'")
        .fetch_one(&pool)
        .await?;
    let chunk_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = 'mchk_idem_1' AND type = 'MemoryChunk'")
        .fetch_one(&pool)
        .await?;
    let has_chunk: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE from_id = 'mem_idem_1' AND edge_type = 'HAS_CHUNK' AND to_id = 'mchk_idem_1'")
        .fetch_one(&pool)
        .await?;

    assert_eq!(memory_nodes, 1);
    assert_eq!(chunk_nodes, 1);
    assert_eq!(has_chunk, 1);

    Ok(())
}
