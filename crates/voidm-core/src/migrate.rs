use anyhow::Result;
use sqlx::SqlitePool;

/// Run all migrations: memories, memory_scopes, db_meta, and all graph_* tables.
/// Idempotent — safe to run on every startup.
pub async fn run(pool: &SqlitePool) -> Result<()> {
    sqlx::query(SCHEMA).execute(pool).await?;
    upgrade_add_quality_score(pool).await?;
    upgrade_add_concept_type(pool).await?;
    upgrade_add_search_sessions(pool).await?;
    upgrade_add_search_session_summary(pool).await?;
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

/// Add concept_type column to ontology_concepts table
/// Safe to run multiple times (idempotent)
async fn upgrade_add_concept_type(pool: &SqlitePool) -> Result<()> {
    // Check if concept_type column already exists
    let column_exists: (bool,) = sqlx::query_as(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('ontology_concepts') WHERE name = 'concept_type'"
    )
    .fetch_one(pool)
    .await?;

    if !column_exists.0 {
        sqlx::query("ALTER TABLE ontology_concepts ADD COLUMN concept_type TEXT")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_ontology_concepts_type ON ontology_concepts(concept_type)")
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Create search_sessions table for tracking search → get behavior
/// Idempotent — safe to run multiple times
async fn upgrade_add_search_sessions(pool: &SqlitePool) -> Result<()> {
    // Check if search_sessions table already exists
    let table_exists: (bool,) = sqlx::query_as(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='search_sessions'"
    )
    .fetch_one(pool)
    .await?;

    if !table_exists.0 {
        sqlx::query(
            r#"
            CREATE TABLE search_sessions (
                id                TEXT PRIMARY KEY,
                user_id           TEXT NOT NULL,
                query_hash        TEXT NOT NULL,
                started_at        TEXT NOT NULL,
                result_count      INTEGER NOT NULL,
                clicked_results   TEXT,
                last_activity_at  TEXT NOT NULL,
                session_status    TEXT NOT NULL DEFAULT 'open' CHECK (session_status IN ('open', 'closed')),
                closed_at         TEXT,
                created_at        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#
        )
        .execute(pool)
        .await?;

        // Create indexes for efficient queries
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_search_sessions_user_id_status ON search_sessions(user_id, session_status)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_search_sessions_last_activity ON search_sessions(last_activity_at DESC)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_search_sessions_query_hash ON search_sessions(query_hash)")
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// Create search_session_summary table for aggregated session analytics
/// Idempotent — safe to run multiple times
async fn upgrade_add_search_session_summary(pool: &SqlitePool) -> Result<()> {
    // Check if search_session_summary table already exists
    let table_exists: (bool,) = sqlx::query_as(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='search_session_summary'"
    )
    .fetch_one(pool)
    .await?;

    if !table_exists.0 {
        sqlx::query(
            r#"
            CREATE TABLE search_session_summary (
                id                      INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id                 TEXT NOT NULL,
                query_hash              TEXT NOT NULL,
                period_start            TEXT NOT NULL,
                period_end              TEXT NOT NULL,
                total_searches          INTEGER NOT NULL,
                successful_searches     INTEGER NOT NULL,
                total_clicks            INTEGER NOT NULL,
                avg_results_per_session REAL NOT NULL,
                avg_clicks_per_session  REAL NOT NULL,
                success_rate            REAL NOT NULL,
                max_exploration_depth   INTEGER NOT NULL,
                created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(user_id, query_hash, period_start)
            )
            "#
        )
        .execute(pool)
        .await?;

        // Create indexes for efficient queries
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_summary_user_query ON search_session_summary(user_id, query_hash)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_summary_period ON search_session_summary(period_start DESC, period_end DESC)")
            .execute(pool)
            .await?;
        
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_summary_success_rate ON search_session_summary(success_rate DESC)")
            .execute(pool)
            .await?;
    }

    Ok(())
}

const SCHEMA: &str = r#"
-- Core memory storage
CREATE TABLE IF NOT EXISTS memories (
    id            TEXT PRIMARY KEY,
    type          TEXT NOT NULL CHECK (type IN ('episodic','semantic','procedural','conceptual','contextual')),
    content       TEXT NOT NULL,
    importance    INTEGER NOT NULL DEFAULT 5 CHECK (importance BETWEEN 1 AND 10),
    tags          TEXT NOT NULL DEFAULT '[]',
    metadata      TEXT NOT NULL DEFAULT '{}',
    quality_score REAL,
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

-- ── Ontology layer ────────────────────────────────────────────────────────────

-- First-class concept nodes (distinct from memories)
CREATE TABLE IF NOT EXISTS ontology_concepts (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    scope       TEXT,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ontology_concepts_name  ON ontology_concepts(lower(name));
CREATE INDEX IF NOT EXISTS idx_ontology_concepts_scope ON ontology_concepts(scope);

-- FTS for concepts
CREATE VIRTUAL TABLE IF NOT EXISTS ontology_concept_fts USING fts5(
    id UNINDEXED,
    name,
    description,
    tokenize = 'porter ascii'
);

-- Typed edges: concept↔concept, concept↔memory, memory↔concept
-- from_type / to_type discriminate between 'concept' and 'memory' endpoints
CREATE TABLE IF NOT EXISTS ontology_edges (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id     TEXT NOT NULL,
    from_type   TEXT NOT NULL CHECK (from_type IN ('concept', 'memory')),
    rel_type    TEXT NOT NULL,
    to_id       TEXT NOT NULL,
    to_type     TEXT NOT NULL CHECK (to_type IN ('concept', 'memory')),
    note        TEXT,
    created_at  TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ontology_edges_unique
    ON ontology_edges(from_id, rel_type, to_id);
CREATE INDEX IF NOT EXISTS idx_ontology_edges_from   ON ontology_edges(from_id, rel_type);
CREATE INDEX IF NOT EXISTS idx_ontology_edges_to     ON ontology_edges(to_id, rel_type);
CREATE INDEX IF NOT EXISTS idx_ontology_edges_type   ON ontology_edges(rel_type);

-- NER enrichment tracking: records which memories have been processed
-- by 'voidm ontology enrich-memories' so re-runs skip them by default.
CREATE TABLE IF NOT EXISTS ontology_ner_processed (
    memory_id    TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
    processed_at TEXT NOT NULL,
    entity_count INTEGER NOT NULL DEFAULT 0,
    link_count   INTEGER NOT NULL DEFAULT 0
);

-- ── Batch merge operations (Phase 5) ───────────────────────────────────────

-- Tracks individual merge operations within a batch
CREATE TABLE IF NOT EXISTS ontology_merge_log (
    id                TEXT PRIMARY KEY,
    batch_id          TEXT NOT NULL,
    source_id         TEXT NOT NULL,
    target_id         TEXT NOT NULL,
    edges_retargeted  INTEGER DEFAULT 0,
    conflicts_kept    INTEGER DEFAULT 0,
    status            TEXT NOT NULL CHECK (status IN ('pending', 'completed', 'rolled_back', 'failed')),
    reason            TEXT,
    created_at        TEXT NOT NULL,
    completed_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_merge_log_batch ON ontology_merge_log(batch_id);
CREATE INDEX IF NOT EXISTS idx_merge_log_status ON ontology_merge_log(status);
CREATE INDEX IF NOT EXISTS idx_merge_log_source ON ontology_merge_log(source_id);
CREATE INDEX IF NOT EXISTS idx_merge_log_target ON ontology_merge_log(target_id);

-- Tracks batch merge operations
CREATE TABLE IF NOT EXISTS ontology_merge_batch (
    id               TEXT PRIMARY KEY,
    total_merges     INTEGER NOT NULL,
    failed_merges    INTEGER DEFAULT 0,
    conflicts        INTEGER DEFAULT 0,
    transaction_id   TEXT,
    created_at       TEXT NOT NULL,
    executed_at      TEXT,
    rolled_back_at   TEXT
);

CREATE INDEX IF NOT EXISTS idx_merge_batch_created ON ontology_merge_batch(created_at DESC);

-- ── Telemetry and Feedback (Self-Improvement) ────────────────────────────────

-- Concept usage telemetry: tracks how agents use concepts
CREATE TABLE IF NOT EXISTS concept_telemetry (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    concept_id       TEXT NOT NULL,
    concept_name     TEXT NOT NULL,
    event_type       TEXT NOT NULL CHECK (event_type IN ('query', 'instance_fetch', 'edge_traverse', 'feedback')),
    timestamp        TEXT NOT NULL,
    agent_id         TEXT,
    context          TEXT,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_concept_telemetry_id       ON concept_telemetry(concept_id);
CREATE INDEX IF NOT EXISTS idx_concept_telemetry_event    ON concept_telemetry(event_type);
CREATE INDEX IF NOT EXISTS idx_concept_telemetry_agent    ON concept_telemetry(agent_id);
CREATE INDEX IF NOT EXISTS idx_concept_telemetry_time     ON concept_telemetry(timestamp DESC);

-- Agent feedback on concepts: for self-improvement
CREATE TABLE IF NOT EXISTS agent_feedback (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_id         TEXT NOT NULL,
    feedback_type    TEXT NOT NULL CHECK (feedback_type IN ('helpful', 'duplicate', 'missing', 'contradictory', 'underspecified')),
    concept_id       TEXT,
    concept_name     TEXT NOT NULL,
    message          TEXT,
    timestamp        TEXT NOT NULL,
    context          TEXT,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_agent_feedback_agent      ON agent_feedback(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_feedback_type       ON agent_feedback(feedback_type);
CREATE INDEX IF NOT EXISTS idx_agent_feedback_concept    ON agent_feedback(concept_id);
CREATE INDEX IF NOT EXISTS idx_agent_feedback_timestamp  ON agent_feedback(timestamp DESC);

-- ── User Behavior & Interaction Tracking ────────────────────────────────────

-- Track every user interaction with voidm
CREATE TABLE IF NOT EXISTS user_interactions (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id          TEXT NOT NULL,
    interaction_type TEXT NOT NULL CHECK (interaction_type IN ('search', 'view', 'enrich', 'feedback', 'merge', 'create', 'explore', 'export')),
    target_id        TEXT,
    target_name      TEXT NOT NULL,
    result           TEXT NOT NULL CHECK (result IN ('success', 'skip', 'cancel', 'error')),
    duration_ms      INTEGER DEFAULT 0,
    timestamp        TEXT NOT NULL,
    context          TEXT,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_user_interactions_user_id      ON user_interactions(user_id);
CREATE INDEX IF NOT EXISTS idx_user_interactions_type         ON user_interactions(interaction_type);
CREATE INDEX IF NOT EXISTS idx_user_interactions_target       ON user_interactions(target_id);
CREATE INDEX IF NOT EXISTS idx_user_interactions_timestamp    ON user_interactions(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_user_interactions_result      ON user_interactions(result);

-- User preferences and behavior patterns (computed, not inserted directly)
CREATE TABLE IF NOT EXISTS user_preferences (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id          TEXT NOT NULL UNIQUE,
    favorite_scopes  TEXT,                          -- JSON: [{"scope": "auth", "count": 15}]
    favorite_concepts TEXT,                         -- JSON: [{"name": "JWT", "count": 10}]
    enrichment_rate  REAL DEFAULT 0.5,
    preferred_types  TEXT,                          -- JSON: [{"type": "TECHNIQUE", "freq": 0.6}]
    avg_duration_ms  INTEGER DEFAULT 500,
    peak_hours       TEXT,                          -- JSON: [9, 14, 18]
    work_style       TEXT,                          -- JSON: detailed work style
    last_updated     TEXT NOT NULL,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_user_preferences_user_id ON user_preferences(user_id);
"#;
