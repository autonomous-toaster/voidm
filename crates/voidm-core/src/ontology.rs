use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

// ─── Models ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyEdge {
    pub id: i64,
    pub from_id: String,
    pub from_type: NodeKind,
    pub rel_type: String,
    pub to_id: String,
    pub to_type: NodeKind,
    pub note: Option<String>,
    pub created_at: String,
}

/// Discriminates whether an endpoint in an ontology edge is a Concept or a Memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Concept,
    Memory,
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeKind::Concept => write!(f, "concept"),
            NodeKind::Memory => write!(f, "memory"),
        }
    }
}

impl std::str::FromStr for NodeKind {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "concept" => Ok(NodeKind::Concept),
            "memory" => Ok(NodeKind::Memory),
            other => bail!("Unknown node kind: '{}'", other),
        }
    }
}

/// Ontology-specific edge types (IS_A, INSTANCE_OF, HAS_PROPERTY).
/// Regular EdgeTypes (SUPPORTS, CONTRADICTS, etc.) are also valid in ontology_edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OntologyRelType {
    IsA,
    InstanceOf,
    HasProperty,
    /// Pass-through for any string rel_type (covers existing EdgeType variants too)
    Other(String),
}

impl OntologyRelType {
    pub fn as_str(&self) -> &str {
        match self {
            OntologyRelType::IsA => "IS_A",
            OntologyRelType::InstanceOf => "INSTANCE_OF",
            OntologyRelType::HasProperty => "HAS_PROPERTY",
            OntologyRelType::Other(s) => s.as_str(),
        }
    }

    pub fn all_ontology_types() -> &'static [&'static str] {
        &["IS_A", "INSTANCE_OF", "HAS_PROPERTY"]
    }
}

impl std::fmt::Display for OntologyRelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for OntologyRelType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_uppercase().replace('-', "_").as_str() {
            "IS_A" | "ISA" => Ok(OntologyRelType::IsA),
            "INSTANCE_OF" => Ok(OntologyRelType::InstanceOf),
            "HAS_PROPERTY" => Ok(OntologyRelType::HasProperty),
            other => Ok(OntologyRelType::Other(other.to_string())),
        }
    }
}

// ─── Concept CRUD ─────────────────────────────────────────────────────────────

/// Add a new concept. Returns the created Concept.
pub async fn add_concept(
    pool: &SqlitePool,
    name: &str,
    description: Option<&str>,
    scope: Option<&str>,
) -> Result<Concept> {
    // Check for exact name+scope duplicate
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM ontology_concepts WHERE lower(name) = lower(?) AND (scope IS ? OR (scope IS NULL AND ? IS NULL))"
    )
    .bind(name)
    .bind(scope)
    .bind(scope)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        bail!("Concept '{}' already exists (id: {}). Use 'voidm ontology concept get {}' to inspect it.", name, &id[..8], &id[..8]);
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO ontology_concepts (id, name, description, scope, created_at)
         VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(scope)
    .bind(&now)
    .execute(pool)
    .await
    .context("Failed to insert concept")?;

    // FTS insert
    sqlx::query("INSERT INTO ontology_concept_fts (id, name, description) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(description.unwrap_or(""))
        .execute(pool)
        .await?;

    Ok(Concept { id, name: name.to_string(), description: description.map(str::to_string), scope: scope.map(str::to_string), created_at: now })
}

/// Get a concept by full or short (prefix) ID.
pub async fn get_concept(pool: &SqlitePool, id: &str) -> Result<Concept> {
    let full_id = resolve_concept_id(pool, id).await?;
    let row: (String, String, Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT id, name, description, scope, created_at FROM ontology_concepts WHERE id = ?"
    )
    .bind(&full_id)
    .fetch_one(pool)
    .await
    .with_context(|| format!("Concept '{}' not found", id))?;

    Ok(Concept { id: row.0, name: row.1, description: row.2, scope: row.3, created_at: row.4 })
}

/// List concepts, optionally filtered by scope prefix.
pub async fn list_concepts(
    pool: &SqlitePool,
    scope_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<Concept>> {
    let rows: Vec<(String, String, Option<String>, Option<String>, String)> = if let Some(scope) = scope_filter {
        let prefix = format!("{}%", scope);
        sqlx::query_as(
            "SELECT id, name, description, scope, created_at
             FROM ontology_concepts WHERE scope LIKE ?
             ORDER BY name ASC LIMIT ?"
        )
        .bind(&prefix)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, name, description, scope, created_at
             FROM ontology_concepts ORDER BY name ASC LIMIT ?"
        )
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    };

    Ok(rows.into_iter().map(|(id, name, description, scope, created_at)| Concept { id, name, description, scope, created_at }).collect())
}

/// Delete a concept (and its ontology edges via CASCADE).
pub async fn delete_concept(pool: &SqlitePool, id: &str) -> Result<bool> {
    let full_id = resolve_concept_id(pool, id).await?;

    // FTS: manual delete
    sqlx::query("DELETE FROM ontology_concept_fts WHERE id = ?")
        .bind(&full_id)
        .execute(pool)
        .await?;

    let result = sqlx::query("DELETE FROM ontology_concepts WHERE id = ?")
        .bind(&full_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

// ─── Ontology Edge CRUD ───────────────────────────────────────────────────────

/// Add an edge in the ontology graph. Both endpoints can be concepts or memories.
pub async fn add_ontology_edge(
    pool: &SqlitePool,
    from_id: &str,
    from_kind: NodeKind,
    rel_type: &OntologyRelType,
    to_id: &str,
    to_kind: NodeKind,
    note: Option<&str>,
) -> Result<OntologyEdge> {
    // Validate endpoints exist
    validate_node(pool, from_id, &from_kind).await?;
    validate_node(pool, to_id, &to_kind).await?;

    // IS_A and HAS_PROPERTY only make sense between concepts
    match rel_type {
        OntologyRelType::IsA | OntologyRelType::HasProperty => {
            if from_kind != NodeKind::Concept || to_kind != NodeKind::Concept {
                bail!("{} edges must connect two concepts", rel_type.as_str());
            }
        }
        OntologyRelType::InstanceOf => {
            if to_kind != NodeKind::Concept {
                bail!("INSTANCE_OF target must be a concept");
            }
        }
        _ => {}
    }

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO ontology_edges
         (from_id, from_type, rel_type, to_id, to_type, note, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(from_id)
    .bind(from_kind.to_string())
    .bind(rel_type.as_str())
    .bind(to_id)
    .bind(to_kind.to_string())
    .bind(note)
    .bind(&now)
    .execute(pool)
    .await
    .context("Failed to insert ontology edge")?;

    let id: i64 = sqlx::query_scalar(
        "SELECT id FROM ontology_edges WHERE from_id = ? AND to_id = ? AND rel_type = ?"
    )
    .bind(from_id)
    .bind(to_id)
    .bind(rel_type.as_str())
    .fetch_one(pool)
    .await?;

    Ok(OntologyEdge {
        id,
        from_id: from_id.to_string(),
        from_type: from_kind,
        rel_type: rel_type.as_str().to_string(),
        to_id: to_id.to_string(),
        to_type: to_kind,
        note: note.map(str::to_string),
        created_at: now,
    })
}

/// Remove an ontology edge by id.
pub async fn delete_ontology_edge(pool: &SqlitePool, edge_id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM ontology_edges WHERE id = ?")
        .bind(edge_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// List ontology edges for a node (concept or memory), both directions.
pub async fn list_ontology_edges(
    pool: &SqlitePool,
    node_id: &str,
) -> Result<Vec<OntologyEdge>> {
    let rows: Vec<(i64, String, String, String, String, String, Option<String>, String)> =
        sqlx::query_as(
            "SELECT id, from_id, from_type, rel_type, to_id, to_type, note, created_at
             FROM ontology_edges
             WHERE from_id = ? OR to_id = ?
             ORDER BY created_at ASC"
        )
        .bind(node_id)
        .bind(node_id)
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|(id, from_id, from_type, rel_type, to_id, to_type, note, created_at)| {
            Ok(OntologyEdge {
                id,
                from_id,
                from_type: from_type.parse()?,
                rel_type,
                to_id,
                to_type: to_type.parse()?,
                note,
                created_at,
            })
        })
        .collect()
}

// ─── Hierarchy & Subsumption ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyNode {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub depth: i64,
    pub direction: HierarchyDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HierarchyDirection {
    Ancestor,
    Descendant,
}

/// Return all ancestors (IS_A chain upward) and descendants (IS_A chain downward) of a concept.
pub async fn concept_hierarchy(
    pool: &SqlitePool,
    concept_id: &str,
) -> Result<Vec<HierarchyNode>> {
    let full_id = resolve_concept_id(pool, concept_id).await?;
    let mut results = Vec::new();

    // Ancestors: follow IS_A outgoing edges upward (from → to means "from IS_A to", so go to_id)
    let ancestors: Vec<(String, String, Option<String>, i64)> = sqlx::query_as(
        "WITH RECURSIVE ancestors(id, depth) AS (
           SELECT to_id, 1
           FROM ontology_edges
           WHERE from_id = ? AND rel_type = 'IS_A' AND from_type = 'concept' AND to_type = 'concept'
           UNION ALL
           SELECT e.to_id, a.depth + 1
           FROM ontology_edges e
           JOIN ancestors a ON e.from_id = a.id
           WHERE e.rel_type = 'IS_A' AND e.from_type = 'concept' AND e.to_type = 'concept'
             AND a.depth < 20
         )
         SELECT c.id, c.name, c.description, a.depth
         FROM ancestors a
         JOIN ontology_concepts c ON c.id = a.id
         ORDER BY a.depth ASC"
    )
    .bind(&full_id)
    .fetch_all(pool)
    .await?;

    for (id, name, description, depth) in ancestors {
        results.push(HierarchyNode { id, name, description, depth, direction: HierarchyDirection::Ancestor });
    }

    // Descendants: follow IS_A incoming edges downward (find all x where x IS_A ... root)
    let descendants: Vec<(String, String, Option<String>, i64)> = sqlx::query_as(
        "WITH RECURSIVE descendants(id, depth) AS (
           SELECT from_id, 1
           FROM ontology_edges
           WHERE to_id = ? AND rel_type = 'IS_A' AND from_type = 'concept' AND to_type = 'concept'
           UNION ALL
           SELECT e.from_id, d.depth + 1
           FROM ontology_edges e
           JOIN descendants d ON e.to_id = d.id
           WHERE e.rel_type = 'IS_A' AND e.from_type = 'concept' AND e.to_type = 'concept'
             AND d.depth < 20
         )
         SELECT c.id, c.name, c.description, d.depth
         FROM descendants d
         JOIN ontology_concepts c ON c.id = d.id
         ORDER BY d.depth ASC"
    )
    .bind(&full_id)
    .fetch_all(pool)
    .await?;

    for (id, name, description, depth) in descendants {
        results.push(HierarchyNode { id, name, description, depth, direction: HierarchyDirection::Descendant });
    }

    Ok(results)
}

/// Return all instances of a concept, including instances of all subclasses (full subsumption).
/// Instances are memories or concepts linked via INSTANCE_OF to X or any subclass of X.
pub async fn concept_instances(
    pool: &SqlitePool,
    concept_id: &str,
) -> Result<Vec<ConceptInstance>> {
    let full_id = resolve_concept_id(pool, concept_id).await?;

    // First collect all subclass IDs (including self)
    let subclass_ids: Vec<(String,)> = sqlx::query_as(
        "WITH RECURSIVE subclasses(id) AS (
           SELECT ?
           UNION ALL
           SELECT e.from_id
           FROM ontology_edges e
           JOIN subclasses s ON e.to_id = s.id
           WHERE e.rel_type = 'IS_A' AND e.from_type = 'concept' AND e.to_type = 'concept'
         )
         SELECT id FROM subclasses"
    )
    .bind(&full_id)
    .fetch_all(pool)
    .await?;

    let ids: Vec<String> = subclass_ids.into_iter().map(|(id,)| id).collect();

    // Collect all INSTANCE_OF edges pointing to any of those concept IDs
    let mut instances = Vec::new();
    for cid in &ids {
        let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT from_id, from_type, to_id, note
             FROM ontology_edges
             WHERE to_id = ? AND rel_type = 'INSTANCE_OF'"
        )
        .bind(cid)
        .fetch_all(pool)
        .await?;

        for (from_id, from_type, to_id, note) in rows {
            instances.push(ConceptInstance {
                instance_id: from_id,
                instance_kind: from_type.parse().unwrap_or(NodeKind::Memory),
                concept_id: to_id,
                note,
            });
        }
    }

    Ok(instances)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptInstance {
    pub instance_id: String,
    pub instance_kind: NodeKind,
    pub concept_id: String,
    pub note: Option<String>,
}

// ─── ID resolution ────────────────────────────────────────────────────────────

/// Resolve full or short (prefix, min 4 chars) concept ID.
pub async fn resolve_concept_id(pool: &SqlitePool, id: &str) -> Result<String> {
    let exact: Option<String> = sqlx::query_scalar(
        "SELECT id FROM ontology_concepts WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if let Some(full) = exact {
        return Ok(full);
    }

    if id.len() < 4 {
        bail!("Concept ID prefix '{}' is too short (minimum 4 characters)", id);
    }

    let pattern = format!("{}%", id);
    let matches: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM ontology_concepts WHERE id LIKE ?"
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    match matches.len() {
        0 => bail!("Concept '{}' not found", id),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => bail!(
            "Ambiguous concept ID '{}' matches {} concepts. Use more characters:\n{}",
            id, n,
            matches.iter().map(|m| format!("  {}", m)).collect::<Vec<_>>().join("\n")
        ),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn validate_node(pool: &SqlitePool, id: &str, kind: &NodeKind) -> Result<()> {
    match kind {
        NodeKind::Concept => {
            let exists: Option<String> = sqlx::query_scalar(
                "SELECT id FROM ontology_concepts WHERE id = ?"
            )
            .bind(id)
            .fetch_optional(pool)
            .await?;
            if exists.is_none() {
                bail!("Concept '{}' not found", id);
            }
        }
        NodeKind::Memory => {
            let exists: Option<String> = sqlx::query_scalar(
                "SELECT id FROM memories WHERE id = ?"
            )
            .bind(id)
            .fetch_optional(pool)
            .await?;
            if exists.is_none() {
                bail!("Memory '{}' not found", id);
            }
        }
    }
    Ok(())
}
