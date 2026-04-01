use anyhow::Result;

use crate::models::{AddMemoryRequest, EdgeType, LinkResponse, Memory};
use voidm_scoring;
use crate::redactor;
use crate::config::Config;

/// Convert voidm_core MemoryType to voidm_scoring MemoryType
pub fn convert_memory_type(mt: &crate::models::MemoryType) -> voidm_scoring::MemoryType {
    match mt {
        crate::models::MemoryType::Episodic => voidm_scoring::MemoryType::Episodic,
        crate::models::MemoryType::Semantic => voidm_scoring::MemoryType::Semantic,
        crate::models::MemoryType::Procedural => voidm_scoring::MemoryType::Procedural,
        crate::models::MemoryType::Conceptual => voidm_scoring::MemoryType::Conceptual,
        crate::models::MemoryType::Contextual => voidm_scoring::MemoryType::Contextual,
    }
}

/// Resolve a full or short (prefix) ID to a full memory ID (backend-agnostic).
/// 
/// # Backend Abstraction
/// This function accepts any type implementing the Database trait, allowing
/// it to work with SQLite, PostgreSQL, Neo4j, or other backends.
/// 
/// - If `id` is already a full UUID that exists → return it as-is.
/// - If `id` is a prefix → find all matches; error if 0 or >1.
/// - Minimum prefix length: 4 characters.
pub async fn resolve_id<D: voidm_db::Database + ?Sized>(db: &D, id: &str) -> Result<String> {
    db.resolve_memory_id(id).await
}

/// Resolve a full or short (prefix) ID to a full memory ID (SQLite-specific).
/// 

/// Add a memory — full workflow:
/// 1. Compute embedding + quality_score (outside tx)
/// 2. BEGIN tx
/// 3. Insert memory + scopes + FTS + vec + graph node + links
/// 4. COMMIT
/// Returns AddMemoryResponse with suggested_links and duplicate_warning.
// Get a single memory by ID.
/// Get a memory by ID (backend-agnostic via Database trait)
///
/// # Arguments
/// * `db` - Any type implementing Database trait
/// * `id` - Memory ID (full UUID or 4+ char prefix)
///
/// Returns the Memory if found, None if not found
pub async fn get_memory<D: voidm_db::Database + ?Sized>(db: &D, id: &str) -> Result<Option<Memory>> {
    match db.get_memory(id).await? {
        None => Ok(None),
        Some(value) => {
            let memory: Memory = serde_json::from_value(value)?;
            Ok(Some(memory))
        }
    }
}


/// List memories newest-first (backend-agnostic via Database trait)
///
/// # Arguments
/// * `db` - Any type implementing Database trait
/// * `limit` - Maximum number of memories to return
///
/// Returns a list of memories ordered by creation date
pub async fn list_memories<D: voidm_db::Database + ?Sized>(db: &D, limit: Option<usize>) -> Result<Vec<Memory>> {
    let values = db.list_memories(limit).await?;
    let memories: Vec<Memory> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    Ok(memories)
}

/// Delete a memory and all its graph edges (cascade via FK).
/// Delete a memory by ID (backend-agnostic via Database trait)
///
/// # Arguments
/// * `db` - Any type implementing Database trait
/// * `id` - Memory ID (full UUID or 4+ char prefix)
///
/// Returns true if memory was deleted, false if not found
pub async fn delete_memory<D: voidm_db::Database + ?Sized>(db: &D, id: &str) -> Result<bool> {
    db.delete_memory(id).await
}


/// Create a graph edge between two memories.
/// Create a graph edge between two memories (backend-agnostic)
pub async fn link_memories<D: voidm_db::Database + ?Sized>(
    db: &D,
    from_id: &str,
    edge_type: &EdgeType,
    to_id: &str,
    note: Option<&str>,
) -> Result<LinkResponse> {
    // Validate RELATES_TO requires note
    if edge_type.requires_note() && note.is_none() {
        anyhow::bail!("RELATES_TO requires --note explaining why no stronger relationship applies.");
    }

    // Resolve IDs (support short prefixes)
    let from_id = db.resolve_memory_id(from_id).await?;
    let to_id = db.resolve_memory_id(to_id).await?;

    // Call trait method - returns JSON response with resolved IDs
    let response_json = db.link_memories(&from_id, edge_type.as_str(), &to_id, note).await?;
    
    // Convert JSON back to LinkResponse
    let created = response_json.get("created").and_then(|v| v.as_bool()).unwrap_or(false);
    let from = response_json.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let rel = response_json.get("rel").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let to = response_json.get("to").and_then(|v| v.as_str()).unwrap_or("").to_string();

    Ok(LinkResponse {
        created,
        from,
        rel,
        to,
    })
}

/// Redact secrets from memory content, tags, and metadata in-place.
/// Returns list of redaction warnings.
pub fn redact_memory(
    req: &mut AddMemoryRequest,
    config: &Config,
    warnings: &mut Vec<redactor::RedactionWarning>,
) -> Result<()> {
    // Redact content
    let (redacted_content, content_warnings) = redactor::redact_text(&req.content, &config.redaction);
    for mut w in content_warnings {
        w.field = "content".to_string();
        warnings.push(w);
    }
    req.content = redacted_content;

    // Redact tags
    let mut redacted_tags = Vec::new();
    for tag in &req.tags {
        let (redacted_tag, tag_warnings) = redactor::redact_text(tag, &config.redaction);
        for mut w in tag_warnings {
            w.field = "tags".to_string();
            warnings.push(w);
        }
        redacted_tags.push(redacted_tag);
    }
    req.tags = redacted_tags;

    // Redact metadata
    if let Ok(metadata_str) = serde_json::to_string(&req.metadata) {
        let (redacted_metadata_str, metadata_warnings) = redactor::redact_text(&metadata_str, &config.redaction);
        for mut w in metadata_warnings {
            w.field = "metadata".to_string();
            warnings.push(w);
        }
        if let Ok(redacted_metadata) = serde_json::from_str(&redacted_metadata_str) {
            req.metadata = redacted_metadata;
        }
    }

    Ok(())
}

// Trait-based wrappers for CLI compatibility
// These allow commands to use &Arc<dyn Database> while maintaining backward compat

