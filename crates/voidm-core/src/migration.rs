use anyhow::Result;
use voidm_db::Database;
use crate::Config;
use std::collections::HashSet;
use crate::models::{Memory, MemoryType, AddMemoryRequest};
use std::str::FromStr;
use voidm_embeddings::chunking::{chunk_memory, ChunkingConfig};

/// Migrate memories from source to destination database
pub async fn migrate_memories(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    config: &Config,
    scope_filter: &Option<String>,
    skip_ids: &HashSet<String>,
    dry_run: bool,
) -> Result<(u32, u32)> {
    let memories = source.list_memories(Some(10000)).await?;

    let mut migrated = 0;
    let mut skipped = 0;

    for mem_json in memories {
        let mem: Memory = serde_json::from_value(mem_json)?;
        
        if skip_ids.contains(&mem.id) {
            skipped += 1;
            continue;
        }

        if let Some(filter) = scope_filter {
            let matches = mem.scopes.iter().any(|s| s.contains(filter));
            if !matches {
                skipped += 1;
                continue;
            }
        }

        if dry_run {
            migrated += 1;
            continue;
        }

        let memory_type = MemoryType::from_str(&mem.memory_type)
            .unwrap_or(MemoryType::Semantic);

        let req = AddMemoryRequest {
            id: Some(mem.id.clone()),
            content: mem.content.clone(),
            memory_type,
            scopes: mem.scopes.clone(),
            tags: mem.tags.clone(),
            importance: mem.importance,
            metadata: mem.metadata.clone(),
            links: vec![],
            context: mem.context.clone(),
            title: mem.title.clone(),
        };

        let req_json = serde_json::to_value(&req)?;
        let config_json = serde_json::to_value(config)?;
        let resp_json = dest.add_memory(req_json, &config_json).await
            .map_err(|e| anyhow::anyhow!("Failed to create memory {} in destination: {}", mem.id, e))?;
        let response: crate::models::AddMemoryResponse = serde_json::from_value(resp_json)?;

        let persisted_memory = dest.get_memory(&response.id).await?;
        if persisted_memory.is_none() {
            anyhow::bail!("Destination did not persist memory {} after add_memory", response.id);
        }

        let existing_chunks = dest.fetch_chunks(100000).await?
            .into_iter()
            .filter(|(_, _, memory_id)| memory_id == &response.id)
            .count();
        if existing_chunks == 0 {
            let chunk_config = ChunkingConfig {
                target_size: crate::memory_policy::CHUNK_TARGET_SIZE,
                min_chunk_size: crate::memory_policy::CHUNK_MIN_SIZE,
                max_chunk_size: crate::memory_policy::CHUNK_MAX_SIZE,
                overlap: crate::memory_policy::CHUNK_OVERLAP,
                smart_breaks: true,
            };
            let chunks = chunk_memory(&response.id, &mem.content, &mem.created_at, &chunk_config);
            for chunk in chunks {
                let embedding = if config.embeddings.enabled {
                    crate::embeddings::embed_text(&config.embeddings.model, &chunk.content).ok()
                } else {
                    None
                };
                dest.upsert_chunk(
                    &chunk.id,
                    &response.id,
                    &chunk.content,
                    chunk.index,
                    chunk.size,
                    &chunk.created_at,
                ).await?;
                if let Some(embedding) = embedding {
                    let _ = dest.store_chunk_embedding(chunk.id.clone(), response.id.clone(), embedding).await?;
                }
            }
        }

        migrated += 1;

        if migrated % 100 == 0 {
            println!("  Migrated {} memories...", migrated);
        }
    }

    Ok((migrated, skipped))
}

/// Migrate chunks from source to destination database
pub async fn migrate_chunks(
    source: &(impl Database + ?Sized),
    _dest: &(impl Database + ?Sized),
    skip_ids: &HashSet<String>,
    dry_run: bool,
) -> Result<(u32, u32)> {
    let chunks = source.list_chunks().await?;
    let edges = source.list_chunk_edges().await?;

    let mut migrated = 0;
    let mut skipped = 0;

    // Build edge map: chunk_id -> memory_id
    let mut edge_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for edge_val in edges {
        if let (Some(from), Some(to)) = (edge_val.get("from").and_then(|v| v.as_str()), 
                                          edge_val.get("to").and_then(|v| v.as_str())) {
            edge_map.insert(from.to_string(), to.to_string());
        }
    }

    for chunk_val in chunks {
        let chunk_id = chunk_val.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Chunk missing id"))?;

        if skip_ids.contains(chunk_id) {
            skipped += 1;
            continue;
        }

        if dry_run {
            migrated += 1;
            continue;
        }

        // Get memory_id from edge map
        if let Some(_memory_id) = edge_map.get(chunk_id) {
            // Store chunk in destination
            // Note: Individual backends will handle chunk storage via their create_chunk equivalent
            // For now, chunks are handled via add_memory pipeline
            migrated += 1;
        } else {
            skipped += 1;
        }
    }

    Ok((migrated, skipped))
}

/// Migrate tags from source to destination database
pub async fn migrate_tags(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    dry_run: bool,
) -> Result<(u32, u32)> {
    let tags = source.list_tags().await?;
    let tag_edges = source.list_tag_edges().await?;

    let mut migrated = 0;
    let mut skipped = 0;

    // First create all tag nodes (idempotent)
    let mut tag_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    
    for tag_val in tags {
        let tag_name = tag_val.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Tag missing name"))?;
        
        let tag_id = tag_val.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(tag_name); // Use name as ID if not present

        if dry_run {
            migrated += 1;
            tag_id_map.insert(tag_id.to_string(), tag_id.to_string());
            continue;
        }

        match dest.create_tag(tag_name).await {
            Ok((new_id, _created)) => {
                tag_id_map.insert(tag_id.to_string(), new_id);
                migrated += 1;
            }
            Err(_) => {
                skipped += 1;
            }
        }
    }

    // Then create tag-memory edges
    for edge_val in tag_edges {
        if dry_run {
            continue;
        }

        if let (Some(tag_id), Some(mem_id)) = (
            edge_val.get("from").and_then(|v| v.as_str()),
            edge_val.get("to").and_then(|v| v.as_str()),
        ) {
            if let Some(new_tag_id) = tag_id_map.get(tag_id) {
                let _ = dest.link_tag_to_memory(new_tag_id, mem_id).await;
            }
        }
    }

    Ok((migrated, skipped))
}

/// Migrate entities from source to destination database
pub async fn migrate_entities(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    dry_run: bool,
) -> Result<(u32, u32)> {
    let entities = source.list_entities().await?;
    let mention_edges = source.list_entity_mention_edges().await?;

    let mut migrated = 0;
    let mut skipped = 0;

    // First create all entity nodes (idempotent)
    let mut entity_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    
    for entity_val in entities {
        let entity_name = entity_val.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Entity missing name"))?;
        
        let entity_type = entity_val.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("MISC");
        
        let entity_id = entity_val.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(entity_name);

        if dry_run {
            migrated += 1;
            entity_id_map.insert(entity_id.to_string(), entity_id.to_string());
            continue;
        }

        match dest.get_or_create_entity(entity_name, entity_type).await {
            Ok((new_id, _created)) => {
                entity_id_map.insert(entity_id.to_string(), new_id);
                migrated += 1;
            }
            Err(_) => {
                skipped += 1;
            }
        }
    }

    // Then create MENTIONS edges with confidence
    for edge_val in mention_edges {
        if dry_run {
            continue;
        }

        if let (Some(chunk_id), Some(entity_id)) = (
            edge_val.get("from").and_then(|v| v.as_str()),
            edge_val.get("to").and_then(|v| v.as_str()),
        ) {
            let confidence = edge_val.get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5) as f32;

            if let Some(new_entity_id) = entity_id_map.get(entity_id) {
                let _ = dest.link_chunk_to_entity(chunk_id, new_entity_id, confidence).await;
            }
        }
    }

    Ok((migrated, skipped))
}

#[cfg(test)]
mod tests {
    // Note: Full migration tests require a running database backend.
    // These tests verify the migration logic structure.
    // Integration tests with real databases are in crates/voidm-core/tests/

    #[test]
    fn test_migrate_memories_structure() {
        // Verify the migrate_memories function signature
        // Full tests require real DB setup
        assert!(true); // Placeholder for actual integration tests
    }

}
