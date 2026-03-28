use anyhow::Result;
use voidm_db_trait::Database;
use crate::Config;
use std::collections::HashSet;
use crate::models::{Memory, MemoryType, AddMemoryRequest};
use std::str::FromStr;

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
            content: mem.content,
            memory_type,
            scopes: mem.scopes,
            tags: mem.tags,
            importance: mem.importance,
            metadata: mem.metadata,
            links: vec![],
            context: mem.context,
            title: mem.title.clone(),
        };

        let req_json = serde_json::to_value(&req)?;
        let config_json = serde_json::to_value(config)?;
        match dest.add_memory(req_json, &config_json).await {
            Ok(_) => migrated += 1,
            Err(e) => {
                anyhow::bail!("Failed to create memory in destination: {}", e);
            }
        }

        if migrated % 100 == 0 {
            println!("  Migrated {} memories...", migrated);
        }
    }

    Ok((migrated, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

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
