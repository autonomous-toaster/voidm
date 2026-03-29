use anyhow::Result;
use sqlx::SqlitePool;

/// Run all migrations: memories, memory_scopes, db_meta, and all graph_* tables.
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

-- Core memory storage (legacy, to be migrated)
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
    content,
    tokenize = 'porter ascii'
);

-- Scopes: many-to-many, prefix-match filtered
CREATE TABLE IF NOT EXISTS memory_scopes (
    memory_id   TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    scope       TEXT NOT NULL,
    PRIMARY KEY (memory_id, scope)
);

CREATE INDEX IF NOT EXISTS idx_memory_scopes_scope ON memory_scopes(scope);

-- DB-level metadata (embedding model, dimension, schema version)
CREATE TABLE IF NOT EXISTS db_meta (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);

-- Graph: EAV schema (all prefixed graph_)
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

-- Labels
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
