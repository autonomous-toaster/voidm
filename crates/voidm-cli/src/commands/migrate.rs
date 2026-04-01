use anyhow::{Context, Result};
use clap::Parser;
use std::sync::Arc;
use voidm_db::Database;
use std::io::Write;

#[derive(Parser, Clone)]
pub struct MigrateArgs {
    /// Source backend: 'sqlite' or 'neo4j'
    #[arg(long, value_name = "SOURCE")]
    pub from: String,

    /// Destination backend: 'sqlite' or 'neo4j'
    #[arg(long, value_name = "DEST")]
    pub to: String,

    /// Only migrate scopes matching this pattern (optional)
    #[arg(long)]
    pub scope_filter: Option<String>,

    /// Dry run: show what would be migrated without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Skip memories with these IDs (comma-separated)
    #[arg(long)]
    pub skip_ids: Option<String>,

    /// Force update ALL existing records (including already-migrated ones)
    /// Useful when schema changes and you need to backfill new fields
    #[arg(long)]
    pub update_all: bool,

    /// Clean target database before migration (DELETE all known nodes and edges)
    /// Requires --confirm to actually delete (for safety)
    #[arg(long)]
    pub clean: bool,

    /// Confirm deletion when using --clean (skips confirmation prompt)
    #[arg(long)]
    pub confirm: bool,

    /// Skip validation after migration
    #[arg(long)]
    pub skip_validation: bool,
}

pub async fn run(args: MigrateArgs, config: &voidm_core::Config, cli_db: Option<&str>, json: bool) -> Result<()> {
    // Validate backends
    let from_backend = args.from.to_lowercase();
    let to_backend = args.to.to_lowercase();

    if from_backend == to_backend && !args.update_all {
        anyhow::bail!("Source and destination backends cannot be the same (use --update-all to refresh schema on same backend)");
    }

    if ![from_backend.as_str(), to_backend.as_str()].iter().all(|b| matches!(*b, "sqlite" | "neo4j")) {
        anyhow::bail!("Backend must be 'sqlite' or 'neo4j'");
    }

    // Parse skip list
    let skip_ids: std::collections::HashSet<String> = args
        .skip_ids
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let sqlite_path = config.db_path(cli_db);

    // Open source database
    let source_db: Arc<dyn voidm_db::Database> = if from_backend == "sqlite" {
        let pool = voidm_sqlite::open_pool(&sqlite_path).await?;
        Arc::new(voidm_sqlite::SqliteDatabase::new(pool))
    } else {
        let neo4j_config = config.database.neo4j.as_ref()
            .context("Neo4j config missing for source")?;
        Arc::new(voidm_neo4j::Neo4jDatabase::connect(
            &neo4j_config.uri,
            &neo4j_config.username,
            &neo4j_config.password,
            &neo4j_config.database,
        ).await?)
    };

    // Open destination database
    let dest_db: Arc<dyn voidm_db::Database> = if to_backend == "sqlite" {
        let pool = voidm_sqlite::open_pool(&sqlite_path).await?;
        Arc::new(voidm_sqlite::SqliteDatabase::new(pool))
    } else {
        let neo4j_config = config.database.neo4j.as_ref()
            .context("Neo4j config missing for destination")?;
        Arc::new(voidm_neo4j::Neo4jDatabase::connect(
            &neo4j_config.uri,
            &neo4j_config.username,
            &neo4j_config.password,
            &neo4j_config.database,
        ).await?)
    };

    // Ensure schemas are initialized
    if !args.dry_run {
        source_db.ensure_schema().await?;
        dest_db.ensure_schema().await?;
    }

    // Clean target database if requested
    if args.clean && !args.dry_run {
        if !json {
            println!("🧹 Preparing to clean target database...");
            
            // Get counts before deletion
            let mem_count = dest_db.count_nodes("Memory").await.unwrap_or(0);
            let chunk_count = dest_db.count_nodes("MemoryChunk").await.unwrap_or(0);
            let tag_count = dest_db.count_nodes("Tag").await.unwrap_or(0);
            let entity_count = dest_db.count_nodes("Entity").await.unwrap_or(0);
            let edge_count = dest_db.count_edges(None).await.unwrap_or(0);

            println!("\n⚠️  WARNING: This will DELETE all data in the target database!");
            println!("\nWill delete:");
            println!("  - {} Memory nodes", mem_count);
            println!("  - {} MemoryChunk nodes", chunk_count);
            println!("  - {} Tag nodes", tag_count);
            println!("  - {} Entity nodes", entity_count);
            println!("  - {} relationships/edges", edge_count);

            if !args.confirm {
                print!("\nContinue? [y/N]: ");
                std::io::stdout().flush()?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
        }

        dest_db.clean_database().await?;
        if !json {
            println!("✓ Database cleaned");
        }
    }

    if !json {
        println!("\n📤 Exporting from {}...", from_backend);
    }

    // Migrate memories
    let (mem_migrated, mem_skipped) = migrate_memories(source_db.as_ref(), dest_db.as_ref(), config, &args.scope_filter, &skip_ids, args.dry_run, args.update_all, json).await?;

    // Migrate chunks
    let (chunk_migrated, chunk_skipped) = migrate_chunks(source_db.as_ref(), dest_db.as_ref(), &skip_ids, args.dry_run, json).await?;

    // Migrate tags
    let (tag_migrated, tag_skipped) = migrate_tags(source_db.as_ref(), dest_db.as_ref(), args.dry_run, json).await?;

    // Migrate entities
    let (entity_migrated, entity_skipped) = migrate_entities(source_db.as_ref(), dest_db.as_ref(), args.dry_run, json).await?;

    // Migrate relationships (memory-to-memory edges)
    let (edge_migrated, edge_skipped) = migrate_relationships(source_db.as_ref(), dest_db.as_ref(), &skip_ids, args.dry_run, json).await?;

    // Migrate tag edges
    let (tag_edge_migrated, tag_edge_skipped) = migrate_tag_edges(source_db.as_ref(), dest_db.as_ref(), args.dry_run, json).await?;

    // Migrate entity mention edges
    let (entity_edge_migrated, entity_edge_skipped) = migrate_entity_edges(source_db.as_ref(), dest_db.as_ref(), args.dry_run, json).await?;

    // Validation
    if !args.skip_validation && !args.dry_run {
        if !json {
            println!("\n✓ Validating migration...");
        }
        validate_migration(source_db.as_ref(), dest_db.as_ref(), json).await?;
    }

    if !json {
        println!("\n📊 Migration Summary:");
        println!("  Memories:       {} migrated, {} skipped", mem_migrated, mem_skipped);
        println!("  Chunks:         {} migrated, {} skipped", chunk_migrated, chunk_skipped);
        println!("  Tags:           {} migrated, {} skipped", tag_migrated, tag_skipped);
        println!("  Entities:       {} migrated, {} skipped", entity_migrated, entity_skipped);
        println!("  Edges:          {} migrated, {} skipped", edge_migrated, edge_skipped);
        println!("  Tag Edges:      {} migrated, {} skipped", tag_edge_migrated, tag_edge_skipped);
        println!("  Entity Edges:   {} migrated, {} skipped", entity_edge_migrated, entity_edge_skipped);
        println!("\n✓ Migration complete!");
    } else {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "status": "success",
            "source_backend": from_backend,
            "destination_backend": to_backend,
            "memories": {"migrated": mem_migrated, "skipped": mem_skipped},
            "chunks": {"migrated": chunk_migrated, "skipped": chunk_skipped},
            "tags": {"migrated": tag_migrated, "skipped": tag_skipped},
            "entities": {"migrated": entity_migrated, "skipped": entity_skipped},
            "edges": {"migrated": edge_migrated, "skipped": edge_skipped},
            "tag_edges": {"migrated": tag_edge_migrated, "skipped": tag_edge_skipped},
            "entity_edges": {"migrated": entity_edge_migrated, "skipped": entity_edge_skipped}
        }))?);
    }

    Ok(())
}

async fn migrate_memories(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    config: &voidm_core::Config,
    scope_filter: &Option<String>,
    skip_ids: &std::collections::HashSet<String>,
    dry_run: bool,
    _update_all: bool,
    json: bool,
) -> Result<(u32, u32)> {
    let memories_json = source.list_memories(Some(10000)).await?;
    let mut memories = Vec::new();
    for mem_json in memories_json {
        if let Ok(mem) = serde_json::from_value::<voidm_db::models::Memory>(mem_json) {
            memories.push(mem);
        }
    }

    let mut migrated = 0;
    let mut skipped = 0;

    for mem in memories {
        // Skip if in skip list
        if skip_ids.contains(&mem.id) {
            skipped += 1;
            continue;
        }

        // Filter by scope if specified
        if let Some(filter) = scope_filter {
            if !mem.scopes.iter().any(|s| s.contains(filter)) {
                skipped += 1;
                continue;
            }
        }

        if dry_run {
            migrated += 1;
            if !json {
                println!("  [DRY RUN] Would migrate memory: {} ({})", mem.id, mem.memory_type);
            }
            continue;
        }

        let req = voidm_db::models::AddMemoryRequest {
            id: Some(mem.id.clone()),
            content: mem.content.clone(),
            memory_type: mem.memory_type.parse()?,
            scopes: mem.scopes.clone(),
            tags: mem.tags.clone(),
            importance: mem.importance,
            metadata: mem.metadata.clone(),
            links: vec![],
            context: mem.context,
            title: mem.title,
        };

        let req_json = serde_json::to_value(&req)?;
        let config_json = serde_json::to_value(config)?;
        let resp_json = dest.add_memory(req_json, &config_json).await?;
        let response: voidm_db::models::AddMemoryResponse = serde_json::from_value(resp_json)
            .context("Failed to parse add_memory response during migration")?;
        let persisted = dest.get_memory(&response.id).await?
            .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(|s| s == response.id))
            .unwrap_or(false);
        if !persisted {
            anyhow::bail!("Destination did not persist memory {} after add_memory", response.id);
        }

        let existing_chunks = dest.fetch_chunks(100000).await?
            .into_iter()
            .filter(|(_, _, memory_id)| memory_id == &response.id)
            .count();
        if existing_chunks == 0 {
            let chunk_config = voidm_core::embeddings::ChunkingConfig {
                target_size: voidm_core::memory_policy::CHUNK_TARGET_SIZE,
                min_chunk_size: voidm_core::memory_policy::CHUNK_MIN_SIZE,
                max_chunk_size: voidm_core::memory_policy::CHUNK_MAX_SIZE,
                overlap: voidm_core::memory_policy::CHUNK_OVERLAP,
                smart_breaks: true,
            };
            let chunks = voidm_core::embeddings::chunk_memory(&response.id, &mem.content, &mem.created_at, &chunk_config);
            for chunk in chunks {
                dest.upsert_chunk(
                    &chunk.id,
                    &response.id,
                    &chunk.content,
                    chunk.index,
                    chunk.size,
                    &chunk.created_at,
                ).await?;
                if config.embeddings.enabled {
                    if let Ok(embedding) = voidm_core::embeddings::embed_text(&config.embeddings.model, &chunk.content) {
                        let _ = dest.store_chunk_embedding(chunk.id.clone(), response.id.clone(), embedding).await?;
                    }
                }
            }
        }

        migrated += 1;

        if !json && migrated % 100 == 0 {
            println!("  Migrated {} memories...", migrated);
        }
    }

    if !json {
        println!("✓ Memories: {} migrated, {} skipped", migrated, skipped);
    }

    Ok((migrated, skipped))
}

async fn migrate_chunks(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    skip_ids: &std::collections::HashSet<String>,
    dry_run: bool,
    json: bool,
) -> Result<(u32, u32)> {
    let (migrated, skipped) = voidm_core::migration::migrate_chunks(source, dest, skip_ids, dry_run).await?;
    if !json {
        println!("✓ Chunks: {} migrated, {} skipped", migrated, skipped);
    }
    Ok((migrated, skipped))
}

async fn migrate_tags(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    dry_run: bool,
    json: bool,
) -> Result<(u32, u32)> {
    let (migrated, skipped) = voidm_core::migration::migrate_tags(source, dest, dry_run).await?;
    if !json {
        println!("✓ Tags: {} migrated, {} skipped", migrated, skipped);
    }
    Ok((migrated, skipped))
}

async fn migrate_entities(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    dry_run: bool,
    json: bool,
) -> Result<(u32, u32)> {
    let (migrated, skipped) = voidm_core::migration::migrate_entities(source, dest, dry_run).await?;
    if !json {
        println!("✓ Entities: {} migrated, {} skipped", migrated, skipped);
    }
    Ok((migrated, skipped))
}

async fn migrate_relationships(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    skip_ids: &std::collections::HashSet<String>,
    dry_run: bool,
    json: bool,
) -> Result<(u32, u32)> {
    let edges_json = source.list_edges().await?;
    let mut edges = Vec::new();
    for edge_json in edges_json {
        if let Ok(edge) = serde_json::from_value::<voidm_db::models::MemoryEdge>(edge_json) {
            edges.push(edge);
        }
    }

    let mut migrated = 0;
    let mut skipped = 0;

    for edge in edges {
        // Skip if either endpoint is in skip list
        if skip_ids.contains(&edge.from_id) || skip_ids.contains(&edge.to_id) {
            skipped += 1;
            continue;
        }

        if dry_run {
            migrated += 1;
            if !json {
                println!("  [DRY RUN] Would migrate edge: {} -> {} ({})", edge.from_id, edge.to_id, edge.rel_type);
            }
            continue;
        }

        // Parse edge type from string
        let rel_type = match edge.rel_type.as_str() {
            "SUPPORTS" => voidm_db::models::EdgeType::Supports,
            "CONTRADICTS" => voidm_db::models::EdgeType::Contradicts,
            "PRECEDES" => voidm_db::models::EdgeType::Precedes,
            "DERIVED_FROM" => voidm_db::models::EdgeType::DerivedFrom,
            "RELATES_TO" => voidm_db::models::EdgeType::RelatesTo,
            "EXEMPLIFIES" => voidm_db::models::EdgeType::Exemplifies,
            "PART_OF" => voidm_db::models::EdgeType::PartOf,
            _ => {
                if !json {
                    eprintln!("  Warning: Unknown edge type '{}', skipping edge {} -> {}", edge.rel_type, edge.from_id, edge.to_id);
                }
                skipped += 1;
                continue;
            }
        };

        match dest.link_memories(&edge.from_id, rel_type.as_str(), &edge.to_id, edge.note.as_deref()).await {
            Ok(resp_json) => {
                let resp: voidm_db::models::LinkResponse = match serde_json::from_value(resp_json) {
                    Ok(r) => r,
                    Err(_) => {
                        if !json {
                            eprintln!("  ERROR: Failed to parse response for edge {} -> {}", edge.from_id, edge.to_id);
                        }
                        skipped += 1;
                        continue;
                    }
                };
                if resp.created {
                    migrated += 1;
                    if !json && migrated % 10 == 0 {
                        println!("  Migrated {} edges...", migrated);
                    }
                } else {
                    if !json {
                        eprintln!("  ERROR: Edge NOT created (MATCH failed?) {} -> {} ({})", edge.from_id, edge.to_id, edge.rel_type);
                    }
                    skipped += 1;
                }
            }
            Err(e) => {
                if !json {
                    eprintln!("  ERROR: {} -> {} ({}): {}", edge.from_id, edge.to_id, edge.rel_type, e);
                }
                skipped += 1;
            }
        }
    }

    if !json {
        println!("✓ Edges: {} migrated, {} skipped", migrated, skipped);
    }

    Ok((migrated, skipped))
}

async fn migrate_tag_edges(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    dry_run: bool,
    json: bool,
) -> Result<(u32, u32)> {
    let tag_edges = source.list_tag_edges().await?;
    let mut migrated = 0;
    let mut skipped = 0;

    for edge_val in tag_edges {
        if dry_run {
            migrated += 1;
            continue;
        }

        if let (Some(tag_id), Some(mem_id)) = (
            edge_val.get("from").and_then(|v| v.as_str()),
            edge_val.get("to").and_then(|v| v.as_str()),
        ) {
            match dest.link_tag_to_memory(tag_id, mem_id).await {
                Ok(true) => migrated += 1,
                _ => skipped += 1,
            }
        } else {
            skipped += 1;
        }
    }

    if !json {
        println!("✓ Tag Edges: {} migrated, {} skipped", migrated, skipped);
    }

    Ok((migrated, skipped))
}

async fn migrate_entity_edges(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    dry_run: bool,
    json: bool,
) -> Result<(u32, u32)> {
    let mention_edges = source.list_entity_mention_edges().await?;
    let mut migrated = 0;
    let mut skipped = 0;

    for edge_val in mention_edges {
        if dry_run {
            migrated += 1;
            continue;
        }

        if let (Some(chunk_id), Some(entity_id)) = (
            edge_val.get("from").and_then(|v| v.as_str()),
            edge_val.get("to").and_then(|v| v.as_str()),
        ) {
            let confidence = edge_val.get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5) as f32;

            match dest.link_chunk_to_entity(chunk_id, entity_id, confidence).await {
                Ok(true) => migrated += 1,
                _ => skipped += 1,
            }
        } else {
            skipped += 1;
        }
    }

    if !json {
        println!("✓ Entity Edges: {} migrated, {} skipped", migrated, skipped);
    }

    Ok((migrated, skipped))
}

async fn validate_migration(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    json: bool,
) -> Result<()> {
    let src_mem = source.count_nodes("Memory").await.unwrap_or(0);
    let dst_mem = dest.count_nodes("Memory").await.unwrap_or(0);

    let src_memories = source.list_memories(Some(100_000)).await.unwrap_or_default();
    let dst_memories = dest.list_memories(Some(100_000)).await.unwrap_or_default();
    let src_memory_ids: std::collections::HashSet<String> = src_memories.iter()
        .filter_map(|v| v.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()))
        .collect();
    let dst_memory_ids: std::collections::HashSet<String> = dst_memories.iter()
        .filter_map(|v| v.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()))
        .collect();
    let missing_memory_ids: Vec<String> = src_memory_ids.difference(&dst_memory_ids).take(10).cloned().collect();
    let extra_memory_ids: Vec<String> = dst_memory_ids.difference(&src_memory_ids).take(10).cloned().collect();

    let src_chunk = source.count_nodes("MemoryChunk").await.unwrap_or(0);
    let dst_chunk = dest.count_nodes("MemoryChunk").await.unwrap_or(0);
    let chunk_backfilled = src_chunk == 0 && dst_chunk >= src_mem;

    let src_tag = source.count_nodes("Tag").await.unwrap_or(0);
    let dst_tag = dest.count_nodes("Tag").await.unwrap_or(0);
    let src_tag_edges = source.count_edges(Some("HAS_TAG")).await.unwrap_or(0);
    let dst_tag_edges = dest.count_edges(Some("HAS_TAG")).await.unwrap_or(0);
    let tags_reconstructed = src_tag == 0 && src_tag_edges == 0 && dst_tag > 0 && dst_tag_edges > 0;

    let src_entity = source.count_nodes("Entity").await.unwrap_or(0);
    let dst_entity = dest.count_nodes("Entity").await.unwrap_or(0);

    let mem_ok = src_mem == dst_mem && missing_memory_ids.is_empty() && extra_memory_ids.is_empty();
    let chunk_ok = src_chunk == dst_chunk || chunk_backfilled;
    let tag_ok = src_tag == dst_tag || tags_reconstructed;
    let entity_ok = src_entity == dst_entity;

    if !json {
        println!("  Memory:       {} → {} {}", src_mem, dst_mem, if mem_ok { "✓" } else { "✗" });
        if !missing_memory_ids.is_empty() {
            println!("    Missing IDs (sample): {}", missing_memory_ids.join(", "));
        }
        if !extra_memory_ids.is_empty() {
            println!("    Extra IDs (sample): {}", extra_memory_ids.join(", "));
        }
        if chunk_backfilled {
            println!("  Chunks:       {} → {} ✓ (backfilled during migration)", src_chunk, dst_chunk);
        } else {
            println!("  Chunks:       {} → {} {}", src_chunk, dst_chunk, if chunk_ok { "✓" } else { "✗" });
        }
        if tags_reconstructed {
            println!("  Tags:         {} → {} ✓ (reconstructed from memory tags/add-flow)", src_tag, dst_tag);
        } else {
            println!("  Tags:         {} → {} {}", src_tag, dst_tag, if tag_ok { "✓" } else { "✗" });
        }
        println!("  Entities:     {} → {} {}", src_entity, dst_entity, if entity_ok { "✓" } else { "✗" });
    }

    let all_match = mem_ok && chunk_ok && tag_ok && entity_ok;
    if !all_match && !json {
        println!("\n⚠️  Warning: Count mismatch detected!");
    }

    if all_match && !json {
        println!("\n✓ All counts match or were intentionally reconstructed!");
    }

    Ok(())
}
