use anyhow::Result;
use sqlx::SqlitePool;

/// Run all migrations: memories, memory_scopes, db_meta, and all graph_* tables.
/// Idempotent — safe to run on every startup.
pub async fn run(pool: &SqlitePool) -> Result<()> {
    sqlx::query(SCHEMA).execute(pool).await?;
    Ok(())
}

const SCHEMA: &str = r#"
-- Core memory storage
CREATE TABLE IF NOT EXISTS memories (
    id          TEXT PRIMARY KEY,
    type        TEXT NOT NULL CHECK (type IN ('episodic','semantic','procedural','conceptual','contextual')),
    content     TEXT NOT NULL,
    importance  INTEGER NOT NULL DEFAULT 5 CHECK (importance BETWEEN 1 AND 10),
    tags        TEXT NOT NULL DEFAULT '[]',
    metadata    TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_type       ON memories(type);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance DESC);

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
