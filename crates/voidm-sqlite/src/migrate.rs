use anyhow::Result;
use sqlx::SqlitePool;

/// Run all migrations for canonical SQLite storage plus temporary compatibility schema.
/// Idempotent — safe to run on every startup.
pub async fn run(pool: &SqlitePool) -> Result<()> {
    // Split SCHEMA by semicolons and execute each statement separately
    // (sqlx::query().execute() only executes the first statement in a multi-statement string)
    for statement in SCHEMA.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        sqlx::query(trimmed).execute(pool).await?;
    }
    upgrade_add_quality_score(pool).await?;
    upgrade_add_context(pool).await?;
    upgrade_add_title(pool).await?;
    backfill_canonical_graph(pool).await?;
    Ok(())
}

async fn backfill_canonical_graph(pool: &SqlitePool) -> Result<()> {
    let has_title: (bool,) = sqlx::query_as(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('memories') WHERE name = 'title'"
    )
    .fetch_one(pool)
    .await?;

    // Canonical Memory nodes from memories table.
    let memory_node_sql = if has_title.0 {
        "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at)
         SELECT id, 'Memory', json_object('title', COALESCE(title, ''), 'content', content), created_at, updated_at
         FROM memories"
    } else {
        "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at)
         SELECT id, 'Memory', json_object('title', '', 'content', content), created_at, updated_at
         FROM memories"
    };
    sqlx::query(memory_node_sql)
        .execute(pool)
        .await?;

    // Reclassify synthetic carrier rows if legacy graph labels reveal a more specific canonical type.
    sqlx::query(
        "UPDATE nodes
         SET type = (
           SELECT COALESCE(
             MAX(CASE WHEN gnl.label = 'MemoryType' THEN 'MemoryType' END),
             MAX(CASE WHEN gnl.label = 'Tag' THEN 'Tag' END),
             MAX(CASE WHEN gnl.label = 'Scope' THEN 'Scope' END),
             MAX(CASE WHEN gnl.label = 'Entity' THEN 'Entity' END),
             MAX(CASE WHEN gnl.label = 'MemoryChunk' THEN 'MemoryChunk' END),
             MAX(CASE WHEN gnl.label = 'Memory' THEN 'Memory' END)
           )
           FROM graph_nodes gn
           JOIN graph_node_labels gnl ON gnl.node_id = gn.id
           WHERE gn.memory_id = nodes.id
         ),
         properties = CASE
           WHEN (
             SELECT COALESCE(
               MAX(CASE WHEN gnl.label = 'MemoryType' THEN 'MemoryType' END),
               MAX(CASE WHEN gnl.label = 'Tag' THEN 'Tag' END),
               MAX(CASE WHEN gnl.label = 'Scope' THEN 'Scope' END),
               MAX(CASE WHEN gnl.label = 'Entity' THEN 'Entity' END)
             )
             FROM graph_nodes gn
             JOIN graph_node_labels gnl ON gnl.node_id = gn.id
             WHERE gn.memory_id = nodes.id
           ) IS NOT NULL
           THEN json_object(
             'name',
             COALESCE((
               SELECT MAX(gnpt.value)
               FROM graph_nodes gn
               JOIN graph_node_props_text gnpt ON gnpt.node_id = gn.id
               JOIN graph_property_keys gpk ON gpk.id = gnpt.key_id
               WHERE gn.memory_id = nodes.id AND gpk.key = 'name'
             ), '')
           )
           ELSE properties
         END
         WHERE EXISTS (SELECT 1 FROM graph_nodes gn WHERE gn.memory_id = nodes.id)"
    )
    .execute(pool)
    .await?;

    // Canonical chunk nodes and ownership edges from chunk storage.
    sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at)
         SELECT id, 'MemoryChunk', json_object('text', text, 'memory_id', memory_id, 'index', \"index\", 'break_type', break_type), created_at, created_at
         FROM memory_chunks"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at)
         SELECT cme.memory_id || ':HAS_CHUNK:' || cme.chunk_id, cme.memory_id, 'HAS_CHUNK', cme.chunk_id, '{}', COALESCE(mc.created_at, m.created_at)
         FROM chunk_memory_edges cme
         JOIN memory_chunks mc ON mc.id = cme.chunk_id
         JOIN memories m ON m.id = cme.memory_id"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at)
         SELECT memory_id || ':HAS_CHUNK:' || id, memory_id, 'HAS_CHUNK', id, '{}', created_at
         FROM memory_chunks"
    )
    .execute(pool)
    .await?;

    // Backfill first-class scopes into compatibility table from canonical edges if legacy DB only had graph storage.
    sqlx::query(
        "INSERT OR IGNORE INTO memory_scopes (memory_id, scope)
         SELECT e.from_id, json_extract(n.properties, '$.name')
         FROM edges e
         JOIN nodes n ON n.id = e.to_id
         WHERE e.edge_type = 'HAS_SCOPE'
           AND n.type = 'Scope'
           AND json_extract(n.properties, '$.name') IS NOT NULL
           AND json_extract(n.properties, '$.name') <> ''"
    )
    .execute(pool)
    .await?;

    // Backfill from legacy graph_* schema when present.
    sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, type, properties, created_at, updated_at)
         SELECT
           gn.memory_id,
           COALESCE(
             MAX(CASE WHEN gnl.label = 'MemoryType' THEN 'MemoryType' END),
             MAX(CASE WHEN gnl.label = 'Tag' THEN 'Tag' END),
             MAX(CASE WHEN gnl.label = 'Scope' THEN 'Scope' END),
             MAX(CASE WHEN gnl.label = 'Entity' THEN 'Entity' END),
             MAX(CASE WHEN gnl.label = 'MemoryChunk' THEN 'MemoryChunk' END),
             MAX(CASE WHEN gnl.label = 'Memory' THEN 'Memory' END),
             'Memory'
           ),
           CASE
             WHEN COALESCE(
               MAX(CASE WHEN gnl.label = 'MemoryType' THEN 'MemoryType' END),
               MAX(CASE WHEN gnl.label = 'Tag' THEN 'Tag' END),
               MAX(CASE WHEN gnl.label = 'Scope' THEN 'Scope' END),
               MAX(CASE WHEN gnl.label = 'Entity' THEN 'Entity' END)
             ) IS NOT NULL
               THEN json_object('name', COALESCE(MAX(CASE WHEN gpk.key = 'name' THEN gnpt.value END), ''))
             ELSE '{}'
           END,
           COALESCE(m.created_at, CURRENT_TIMESTAMP),
           COALESCE(m.updated_at, COALESCE(m.created_at, CURRENT_TIMESTAMP))
         FROM graph_nodes gn
         LEFT JOIN graph_node_labels gnl ON gnl.node_id = gn.id
         LEFT JOIN graph_node_props_text gnpt ON gnpt.node_id = gn.id
         LEFT JOIN graph_property_keys gpk ON gpk.id = gnpt.key_id
         LEFT JOIN memories m ON m.id = gn.memory_id
         GROUP BY gn.memory_id"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO edges (id, from_id, edge_type, to_id, properties, created_at)
         SELECT
           CASE
             WHEN ge.rel_type = 'BELONGS_TO' THEN dst.memory_id || ':HAS_CHUNK:' || src.memory_id
             ELSE src.memory_id || ':' || ge.rel_type || ':' || dst.memory_id
           END,
           CASE WHEN ge.rel_type = 'BELONGS_TO' THEN dst.memory_id ELSE src.memory_id END,
           CASE WHEN ge.rel_type = 'BELONGS_TO' THEN 'HAS_CHUNK' ELSE ge.rel_type END,
           CASE WHEN ge.rel_type = 'BELONGS_TO' THEN src.memory_id ELSE dst.memory_id END,
           CASE WHEN ge.note IS NOT NULL AND ge.note <> '' THEN json_object('note', ge.note) ELSE '{}' END,
           ge.created_at
         FROM graph_edges ge
         JOIN graph_nodes src ON src.id = ge.source_id
         JOIN graph_nodes dst ON dst.id = ge.target_id"
    )
    .execute(pool)
    .await?;

    // Backfill first-class scopes into compatibility table from legacy graph relationships.
    sqlx::query(
        "INSERT OR IGNORE INTO memory_scopes (memory_id, scope)
         SELECT src.memory_id, COALESCE(MAX(CASE WHEN gpk.key = 'name' THEN gnpt.value END), dst.memory_id)
         FROM graph_edges ge
         JOIN graph_nodes src ON src.id = ge.source_id
         JOIN graph_nodes dst ON dst.id = ge.target_id
         LEFT JOIN graph_node_props_text gnpt ON gnpt.node_id = dst.id
         LEFT JOIN graph_property_keys gpk ON gpk.id = gnpt.key_id
         WHERE ge.rel_type = 'HAS_SCOPE'
         GROUP BY src.memory_id, dst.memory_id"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR REPLACE INTO db_meta (key, value) VALUES ('graph_storage_canonical', 'nodes_edges')"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR REPLACE INTO db_meta (key, value) VALUES ('legacy_graph_backfill_version', '1')"
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR REPLACE INTO db_meta (key, value) VALUES ('legacy_graph_policy', 'migration_input_only')"
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Add quality_score column to existing memories table (Phase 2)
/// Safe to run multiple times (idempotent via IF NOT EXISTS... / PRAGMA table_info)
async fn upgrade_add_quality_score(pool: &SqlitePool) -> Result<()> {
    // Check if quality_score column already exists
    let column_exists: (bool,) = sqlx::query_as(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('memories') WHERE name = 'quality_score'"
    )
    .fetch_one(pool)
    .await?;

    if !column_exists.0 {
        sqlx::query("ALTER TABLE memories ADD COLUMN quality_score REAL")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_quality_score ON memories(quality_score DESC)")
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Add context column to existing memories table (Phase 3)
/// Safe to run multiple times (idempotent via IF NOT EXISTS... / PRAGMA table_info)
async fn upgrade_add_context(pool: &SqlitePool) -> Result<()> {
    // Check if context column already exists
    let column_exists: (bool,) = sqlx::query_as(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('memories') WHERE name = 'context'"
    )
    .fetch_one(pool)
    .await?;

    if !column_exists.0 {
        sqlx::query("ALTER TABLE memories ADD COLUMN context TEXT")
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Add title column to existing memories table
/// Safe to run multiple times (idempotent via IF NOT EXISTS... / PRAGMA table_info)
async fn upgrade_add_title(pool: &SqlitePool) -> Result<()> {
    // Check if title column already exists
    let column_exists: (bool,) = sqlx::query_as(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('memories') WHERE name = 'title'"
    )
    .fetch_one(pool)
    .await?;

    if !column_exists.0 {
        sqlx::query("ALTER TABLE memories ADD COLUMN title TEXT")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_title ON memories(title)")
            .execute(pool)
            .await?;
    }

    Ok(())
}

const SCHEMA: &str = r#"
-- Phase 0: Generic node/edge format (foundation for multi-backend)
CREATE TABLE IF NOT EXISTS nodes (
    id            TEXT PRIMARY KEY,
    type          TEXT NOT NULL,           -- Memory, Chunk, Tag, MemoryType, Scope, Entity, EntityType
    properties    TEXT NOT NULL,           -- JSON: all node-specific data
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);

CREATE TABLE IF NOT EXISTS edges (
    id            TEXT PRIMARY KEY,
    from_id       TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    edge_type     TEXT NOT NULL,           -- HAS_CHUNK, TAGGED_WITH, HAS_TYPE, IN_SCOPE, MENTIONS, RELATED_ENTITY, etc.
    to_id         TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    properties    TEXT,                    -- JSON: metadata (weight, sequence_num, etc.)
    created_at    TEXT NOT NULL,
    UNIQUE(from_id, edge_type, to_id)
);

CREATE INDEX IF NOT EXISTS idx_edges_from_to ON edges(from_id, edge_type, to_id);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);

-- Core memory storage (compatibility storage still used by active flows)
CREATE TABLE IF NOT EXISTS memories (
    id            TEXT PRIMARY KEY,
    type          TEXT NOT NULL CHECK (type IN ('episodic','semantic','procedural','conceptual','contextual')),
    content       TEXT NOT NULL,
    importance    INTEGER NOT NULL DEFAULT 5 CHECK (importance BETWEEN 1 AND 10),
    tags          TEXT NOT NULL DEFAULT '[]',
    metadata      TEXT NOT NULL DEFAULT '{}',
    quality_score REAL,
    context       TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_type          ON memories(type);
CREATE INDEX IF NOT EXISTS idx_memories_created_at    ON memories(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_importance    ON memories(importance DESC);
CREATE INDEX IF NOT EXISTS idx_memories_quality_score ON memories(quality_score DESC);

-- Full-text search virtual table for BM25
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    id UNINDEXED,
    title,
    content,
    tokenize = 'porter ascii'
);

-- Legacy compatibility scope table candidate for future removal.
-- Compatibility policy:
--   - retained for migration/backfill safety
--   - may be reconstructed from canonical `HAS_SCOPE`
--   - must not be treated as canonical truth
CREATE TABLE IF NOT EXISTS memory_scopes (
    memory_id   TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    scope       TEXT NOT NULL,
    PRIMARY KEY (memory_id, scope)
);

CREATE INDEX IF NOT EXISTS idx_memory_scopes_scope ON memory_scopes(scope);

-- Canonical chunk storage
CREATE TABLE IF NOT EXISTS memory_chunks (
    id            TEXT PRIMARY KEY,
    memory_id     TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    text          TEXT NOT NULL,
    "index"       INTEGER NOT NULL,
    size          INTEGER NOT NULL,
    break_type    TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    embedding     BLOB,
    embedding_dim INTEGER
);

CREATE INDEX IF NOT EXISTS idx_memory_chunks_memory_id ON memory_chunks(memory_id, "index");

-- Legacy compatibility table candidate for future removal.
-- Compatibility policy:
--   - retained for migration/backfill safety
--   - should be treated as migration-input / compatibility-only, not canonical truth
-- Canonical chunk ownership is represented via:
--   nodes(type='MemoryChunk')
--   edges(edge_type='HAS_CHUNK')
-- Current audit: active tested runtime paths no longer depend on this table.
CREATE TABLE IF NOT EXISTS chunk_memory_edges (
    chunk_id      TEXT NOT NULL REFERENCES memory_chunks(id) ON DELETE CASCADE,
    memory_id     TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    PRIMARY KEY (chunk_id, memory_id)
);

CREATE INDEX IF NOT EXISTS idx_chunk_memory_edges_memory_id ON chunk_memory_edges(memory_id);

-- DB-level metadata (embedding model, dimension, schema version)
CREATE TABLE IF NOT EXISTS db_meta (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);

-- Legacy compatibility graph schema candidates for future removal.
-- Compatibility policy:
--   - retained for migration/backfill safety
--   - should be treated as migration-input / compatibility-only, not canonical truth
-- Canonical SQLite graph storage is `nodes` / `edges`.
-- Current audit: active tested runtime paths no longer depend on this schema.
CREATE TABLE IF NOT EXISTS graph_nodes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id   TEXT UNIQUE NOT NULL REFERENCES memories(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_graph_nodes_memory_id ON graph_nodes(memory_id);

CREATE TABLE IF NOT EXISTS graph_edges (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id   INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    target_id   INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    rel_type    TEXT NOT NULL,
    note        TEXT,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_graph_edges_source   ON graph_edges(source_id, rel_type);
CREATE INDEX IF NOT EXISTS idx_graph_edges_target   ON graph_edges(target_id, rel_type);
CREATE INDEX IF NOT EXISTS idx_graph_edges_type     ON graph_edges(rel_type);
CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_unique ON graph_edges(source_id, target_id, rel_type);

-- Legacy compatibility labels / property schema below.
CREATE TABLE IF NOT EXISTS graph_node_labels (
    node_id     INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    label       TEXT NOT NULL,
    PRIMARY KEY (node_id, label)
);

CREATE INDEX IF NOT EXISTS idx_graph_node_labels ON graph_node_labels(label, node_id);

-- Property key interning
CREATE TABLE IF NOT EXISTS graph_property_keys (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    key         TEXT UNIQUE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_graph_prop_keys ON graph_property_keys(key);

-- Node property tables (one per type)
CREATE TABLE IF NOT EXISTS graph_node_props_text (
    node_id     INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       TEXT NOT NULL,
    PRIMARY KEY (node_id, key_id)
);

CREATE TABLE IF NOT EXISTS graph_node_props_int (
    node_id     INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       INTEGER NOT NULL,
    PRIMARY KEY (node_id, key_id)
);

CREATE TABLE IF NOT EXISTS graph_node_props_real (
    node_id     INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       REAL NOT NULL,
    PRIMARY KEY (node_id, key_id)
);

CREATE TABLE IF NOT EXISTS graph_node_props_bool (
    node_id     INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       INTEGER NOT NULL CHECK (value IN (0,1)),
    PRIMARY KEY (node_id, key_id)
);

CREATE TABLE IF NOT EXISTS graph_node_props_json (
    node_id     INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       TEXT NOT NULL,
    PRIMARY KEY (node_id, key_id)
);

-- Edge property tables (one per type)
CREATE TABLE IF NOT EXISTS graph_edge_props_text (
    edge_id     INTEGER NOT NULL REFERENCES graph_edges(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       TEXT NOT NULL,
    PRIMARY KEY (edge_id, key_id)
);

CREATE TABLE IF NOT EXISTS graph_edge_props_int (
    edge_id     INTEGER NOT NULL REFERENCES graph_edges(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       INTEGER NOT NULL,
    PRIMARY KEY (edge_id, key_id)
);

CREATE TABLE IF NOT EXISTS graph_edge_props_real (
    edge_id     INTEGER NOT NULL REFERENCES graph_edges(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       REAL NOT NULL,
    PRIMARY KEY (edge_id, key_id)
);

CREATE TABLE IF NOT EXISTS graph_edge_props_bool (
    edge_id     INTEGER NOT NULL REFERENCES graph_edges(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       INTEGER NOT NULL CHECK (value IN (0,1)),
    PRIMARY KEY (edge_id, key_id)
);

CREATE TABLE IF NOT EXISTS graph_edge_props_json (
    edge_id     INTEGER NOT NULL REFERENCES graph_edges(id) ON DELETE CASCADE,
    key_id      INTEGER NOT NULL REFERENCES graph_property_keys(id),
    value       TEXT NOT NULL,
    PRIMARY KEY (edge_id, key_id)
);

-- Indexes on prop tables for fast lookup
CREATE INDEX IF NOT EXISTS idx_graph_node_props_text ON graph_node_props_text(key_id, value, node_id);
CREATE INDEX IF NOT EXISTS idx_graph_node_props_int  ON graph_node_props_int(key_id, value, node_id);
CREATE INDEX IF NOT EXISTS idx_graph_edge_props_text ON graph_edge_props_text(key_id, value, edge_id);

"#;
