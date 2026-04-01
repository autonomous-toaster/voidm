/// Tag-based automatic linking of memories
/// When a new memory is created, find other memories with shared tags
/// and create RELATES_TO edges in the knowledge graph.

use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;

/// Find all memories that share at least one tag with the given memory.
/// Returns list of (memory_id, shared_tag_count, shared_tags_list)
pub async fn find_memories_with_shared_tags(
    db: &Arc<dyn voidm_db::Database>,
    memory_id: &str,
    current_tags: &[String],
) -> Result<Vec<(String, usize, Vec<String>)>> {
    if current_tags.is_empty() {
        return Ok(vec![]);
    }

    // Convert tags to lowercase for case-insensitive comparison
    let current_tags_lower: HashSet<String> = current_tags
        .iter()
        .map(|t| t.to_lowercase())
        .collect();

    // Get all memories with their tags
    //FIXME: use cypher instead of fetching all and filtering in memory
    let all_memories = db.fetch_memories_raw(None, None, 10000).await?;

    let mut results = vec![];

    for (other_id, content) in all_memories {
        if other_id == memory_id {
            continue;
        }

        // Parse memory JSON to extract tags
        let mem: serde_json::Value = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let tags: Vec<String> = mem
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Find shared tags (case-insensitive)
        let other_tags_lower: HashSet<String> = tags
            .iter()
            .map(|t| t.to_lowercase())
            .collect();

        let shared: HashSet<String> = current_tags_lower
            .intersection(&other_tags_lower)
            .cloned()
            .collect();

        if !shared.is_empty() {
            let shared_count = shared.len();
            let shared_list = shared.into_iter().collect::<Vec<_>>();
            results.push((other_id, shared_count, shared_list));
        }
    }

    // Sort by shared tag count (descending)
    results.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(results)
}

/// Check if a link already exists between two memories
pub async fn link_exists(
    db: &Arc<dyn voidm_db::Database>,
    source_id: &str,
    target_id: &str,
) -> Result<bool> {
    // Try to get edges between the two memories
    //FIXME: use cypher instead of fetching all and filtering in memory
    let edges = db.list_edges().await?;
    
    for edge in edges {
        let from = edge.get("from_id").and_then(|v| v.as_str());
        let to = edge.get("to_id").and_then(|v| v.as_str());
        
        if (from == Some(source_id) && to == Some(target_id)) ||
           (from == Some(target_id) && to == Some(source_id)) {
            return Ok(true);
        }
    }
    
    Ok(false)
}

/// Create a RELATES_TO edge between two memories
pub async fn create_tag_link(
    db: &Arc<dyn voidm_db::Database>,
    source_id: &str,
    target_id: &str,
    shared_tags: &[String],
) -> Result<()> {
    // Check if link already exists (both directions)
    let exists = link_exists(db, source_id, target_id).await?;
    if exists {
        return Ok(()); // Link already exists, don't create duplicate
    }

    let note = if shared_tags.is_empty() {
        "Shares tags with memory".to_string()
    } else {
        format!("Shares tags: {}", shared_tags.join(", "))
    };

    db.link_memories(source_id, "RELATES_TO", target_id, Some(&note)).await?;

    tracing::debug!(
        "Created tag-based link: {} → {} (shared tags: {})",
        source_id,
        target_id,
        shared_tags.join(", ")
    );

    Ok(())
}

/// Auto-link a memory to all memories with shared tags (up to limit)
/// Returns count of links created
pub async fn auto_link_by_tags(
    db: &Arc<dyn voidm_db::Database>,
    memory_id: &str,
    tags: &[String],
    max_links: usize,
) -> Result<usize> {
    let matches = find_memories_with_shared_tags(db, memory_id, tags).await?;

    let mut count = 0;
    for (other_id, _shared_count, shared_tags) in matches.iter().take(max_links) {
        create_tag_link(db, memory_id, other_id, &shared_tags).await?;
        count += 1;
    }

    if count > 0 {
        tracing::info!(
            "Auto-linked memory {} to {} other memories by shared tags",
            memory_id,
            count
        );
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_shared_tags() {
        let tags1 = vec!["kubernetes".to_string(), "docker".to_string(), "deployment".to_string()];
        let tags2 = vec!["Docker".to_string(), "containers".to_string()];

        let tags1_lower: HashSet<String> = tags1.iter().map(|t| t.to_lowercase()).collect();
        let tags2_lower: HashSet<String> = tags2.iter().map(|t| t.to_lowercase()).collect();

        let shared: HashSet<String> = tags1_lower.intersection(&tags2_lower).cloned().collect();

        assert_eq!(shared.len(), 1);
        assert!(shared.contains("docker"));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let tag_lower = "kubernetes";
        let tag_upper = "KUBERNETES";
        let tag_mixed = "Kubernetes";

        assert_eq!(tag_lower.to_lowercase(), tag_upper.to_lowercase());
        assert_eq!(tag_lower.to_lowercase(), tag_mixed.to_lowercase());
    }
}
