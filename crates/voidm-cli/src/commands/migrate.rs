use anyhow::{Context, Result};
use clap::Parser;
use std::sync::Arc;
use voidm_core::db::Database;

#[derive(Parser, Clone)]
pub struct MigrateArgs {
    /// Source backend: 'sqlite' or 'neo4j'
    #[arg(value_name = "SOURCE")]
    pub from: String,

    /// Destination backend: 'sqlite' or 'neo4j'
    #[arg(value_name = "DEST")]
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
}

pub async fn run(args: MigrateArgs, config: &voidm_core::Config, cli_db: Option<&str>, json: bool) -> Result<()> {
    // Validate backends
    let from_backend = args.from.to_lowercase();
    let to_backend = args.to.to_lowercase();

    if from_backend == to_backend {
        anyhow::bail!("Source and destination backends cannot be the same");
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

    let sqlite_path = config.db_path(cli_db).to_string_lossy().to_string();

    // Open source database
    let source_db: Arc<dyn voidm_core::db::Database> = if from_backend == "sqlite" {
        let pool = voidm_core::db::sqlite::open_sqlite_pool(&sqlite_path).await?;
        Arc::new(voidm_core::db::sqlite::SqliteDatabase { pool })
    } else {
        let neo4j_config = config.database.neo4j.as_ref()
            .context("Neo4j config missing for source")?;
        Arc::new(voidm_core::db::neo4j::Neo4jDatabase::connect(
            &neo4j_config.uri,
            &neo4j_config.username,
            &neo4j_config.password,
        ).await?)
    };

    // Open destination database
    let dest_db: Arc<dyn voidm_core::db::Database> = if to_backend == "sqlite" {
        let pool = voidm_core::db::sqlite::open_sqlite_pool(&sqlite_path).await?;
        Arc::new(voidm_core::db::sqlite::SqliteDatabase { pool })
    } else {
        let neo4j_config = config.database.neo4j.as_ref()
            .context("Neo4j config missing for destination")?;
        Arc::new(voidm_core::db::neo4j::Neo4jDatabase::connect(
            &neo4j_config.uri,
            &neo4j_config.username,
            &neo4j_config.password,
        ).await?)
    };

    // Ensure schemas are initialized
    if !args.dry_run {
        source_db.ensure_schema().await?;
        dest_db.ensure_schema().await?;
    }

    // Migrate memories
    migrate_memories(source_db.as_ref(), dest_db.as_ref(), config, &args.scope_filter, &skip_ids, args.dry_run, json).await?;

    // Migrate concepts
    migrate_concepts(source_db.as_ref(), dest_db.as_ref(), &args.scope_filter, args.dry_run, json).await?;

    // Migrate relationships (memory-to-memory edges)
    migrate_relationships(source_db.as_ref(), dest_db.as_ref(), &skip_ids, args.dry_run, json).await?;

    // Migrate ontology edges (concept-concept, concept-memory, etc.)
    migrate_ontology_edges(source_db.as_ref(), dest_db.as_ref(), &skip_ids, args.dry_run, json).await?;

    if !json {
        println!("\n✓ Migration complete!");
    } else {
        println!("{}", serde_json::json!({
            "status": "success",
            "message": "Migration complete"
        }));
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
    json: bool,
) -> Result<()> {
    let memories = source.list_memories(Some(10000)).await?;

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

        let req = voidm_core::models::AddMemoryRequest {
            id: Some(mem.id.clone()),
            content: mem.content.clone(),
            memory_type: mem.memory_type.parse()?,
            scopes: mem.scopes.clone(),
            tags: mem.tags.clone(),
            importance: mem.importance,
            metadata: mem.metadata.clone(),
            links: vec![],
        };

        let _ = dest.add_memory(req, config).await?;
        migrated += 1;

        if !json && migrated % 100 == 0 {
            println!("  Migrated {} memories...", migrated);
        }
    }

    if !json {
        println!("Memories: {} migrated, {} skipped", migrated, skipped);
    }

    Ok(())
}

async fn migrate_concepts(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    scope_filter: &Option<String>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let concepts = source.list_concepts(None, 10000).await?;

    let mut migrated = 0;
    let mut skipped = 0;

    for concept in concepts {
        // Filter by scope if specified
        if let Some(filter) = scope_filter {
            if !concept.scope.as_ref().map(|s| s.contains(filter)).unwrap_or(false) {
                skipped += 1;
                continue;
            }
        }

        if dry_run {
            migrated += 1;
            if !json {
                println!("  [DRY RUN] Would migrate concept: {} ({})", concept.id, concept.name);
            }
            continue;
        }

        let _ = dest.add_concept(&concept.name, concept.description.as_deref(), concept.scope.as_deref(), Some(&concept.id)).await?;
        migrated += 1;

        if !json && migrated % 100 == 0 {
            println!("  Migrated {} concepts...", migrated);
        }
    }

    if !json {
        println!("Concepts: {} migrated, {} skipped", migrated, skipped);
    }

    Ok(())
}

async fn migrate_relationships(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    skip_ids: &std::collections::HashSet<String>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let edges = source.list_edges().await?;

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
            "SUPPORTS" => voidm_core::models::EdgeType::Supports,
            "CONTRADICTS" => voidm_core::models::EdgeType::Contradicts,
            "PRECEDES" => voidm_core::models::EdgeType::Precedes,
            "DERIVED_FROM" => voidm_core::models::EdgeType::DerivedFrom,
            "RELATES_TO" => voidm_core::models::EdgeType::RelatesTo,
            "EXEMPLIFIES" => voidm_core::models::EdgeType::Exemplifies,
            "PART_OF" => voidm_core::models::EdgeType::PartOf,
            _ => {
                if !json {
                    eprintln!("  Warning: Unknown edge type '{}', skipping edge {} -> {}", edge.rel_type, edge.from_id, edge.to_id);
                }
                skipped += 1;
                continue;
            }
        };

        match dest.link_memories(&edge.from_id, &rel_type, &edge.to_id, edge.note.as_deref()).await {
            Ok(resp) => {
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
        println!("Edges: {} migrated, {} skipped", migrated, skipped);
    }

    Ok(())
}

async fn migrate_ontology_edges(
    source: &(impl Database + ?Sized),
    dest: &(impl Database + ?Sized),
    skip_ids: &std::collections::HashSet<String>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let edges = source.list_ontology_edges().await?;

    let mut migrated = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for edge in edges {
        // Skip if either endpoint is in skip list
        if skip_ids.contains(&edge.from_id) || skip_ids.contains(&edge.to_id) {
            skipped += 1;
            continue;
        }

        if dry_run {
            migrated += 1;
            if !json {
                println!("  [DRY RUN] Would migrate ontology edge: {} ({}) -> {} ({}) [{}]", 
                    edge.from_id, edge.from_type, edge.to_id, edge.to_type, edge.rel_type);
            }
            continue;
        }

        // Try to create the ontology edge
        match dest.create_ontology_edge(&edge.from_id, &edge.from_type, &edge.rel_type, &edge.to_id, &edge.to_type).await {
            Ok(true) => {
                migrated += 1;
                if !json && migrated % 100 == 0 {
                    println!("  Migrated {} ontology edges...", migrated);
                }
            }
            Ok(false) => {
                if !json && failed < 5 {
                    eprintln!("  Warning: Ontology edge not created: {} -> {}", edge.from_id, edge.to_id);
                }
                failed += 1;
            }
            Err(e) => {
                if !json && failed < 5 {
                    eprintln!("  Error: {} -> {}: {}", edge.from_id, edge.to_id, e);
                }
                failed += 1;
            }
        }
    }

    if !json {
        println!("Ontology Edges: {} migrated, {} failed, {} skipped", migrated, failed, skipped);
    }

    Ok(())
}

async fn try_create_ontology_edge(
    _dest: &(impl Database + ?Sized),
    _edge: &voidm_core::models::OntologyEdgeForMigration,
) -> Result<bool> {
    // For Neo4j, we need direct access to use link_ontology
    // For now, we return Ok(false) indicating not implemented for other backends
    // This is a limitation of the trait-based approach
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voidm_core::db::{Database, SqliteDatabase};
    use std::sync::Arc;

    /// Create a temporary test database in memory
    async fn create_test_db() -> Result<Arc<dyn Database>> {
        let pool = voidm_core::db::sqlite::open_sqlite_pool(":memory:").await?;
        let db = SqliteDatabase { pool };
        db.ensure_schema().await?;
        Ok(Arc::new(db))
    }

    #[tokio::test]
    async fn test_migrate_memories_basic() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create a test memory in source
        let req = voidm_core::models::AddMemoryRequest {
            content: "Test memory".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec![],
            scope: Some("test".to_string()),
            id: None,
        };
        let mem = source.add_memory(&req).await?;
        println!("Created source memory: {}", mem.id);

        // Migrate memories
        let skip_ids = std::collections::HashSet::new();
        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &None, &skip_ids, false, false).await?;

        // Verify memory exists in destination
        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 1, "Should have 1 memory in destination");
        assert_eq!(dest_mems[0].content, "Test memory");
        assert_eq!(dest_mems[0].id, mem.id, "Memory ID should be preserved");

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_memories_preserves_ids() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create memory with specific ID
        let specific_id = "test-id-12345";
        let req = voidm_core::models::AddMemoryRequest {
            content: "Memory with ID".to_string(),
            memory_type: "semantic".to_string(),
            tags: vec!["test".to_string()],
            scope: Some("project/test".to_string()),
            id: Some(specific_id.to_string()),
        };
        let mem = source.add_memory(&req).await?;
        assert_eq!(mem.id, specific_id, "Source should preserve ID");

        // Migrate
        let skip_ids = std::collections::HashSet::new();
        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &None, &skip_ids, false, false).await?;

        // Verify ID preserved in destination
        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems[0].id, specific_id, "Destination should preserve ID");

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_concepts_basic() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create a concept in source with specific ID
        let concept_id = "concept-123";
        let concept = source.add_concept("TestConcept", Some("A test concept"), Some("test"), Some(concept_id)).await?;
        println!("Created source concept: {}", concept.id);

        // Migrate concepts
        migrate_concepts(source.as_ref(), dest.as_ref(), &None, false, false).await?;

        // Verify concept exists in destination with same ID
        let dest_concepts = dest.list_concepts().await?;
        assert_eq!(dest_concepts.len(), 1, "Should have 1 concept in destination");
        assert_eq!(dest_concepts[0].name, "TestConcept");
        assert_eq!(dest_concepts[0].id, concept_id, "Concept ID should be preserved");

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_relationships_basic() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create two memories with specific IDs
        let mem1_id = "mem1";
        let mem2_id = "mem2";
        
        let m1 = source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "First".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec![],
            scope: None,
            id: Some(mem1_id.to_string()),
        }).await?;

        let m2 = source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "Second".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec![],
            scope: None,
            id: Some(mem2_id.to_string()),
        }).await?;

        // Link them
        source.link_memories(&mem1_id, &mem2_id, "SUPPORTS", None).await?;

        // Migrate everything
        let skip_ids = std::collections::HashSet::new();
        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &None, &skip_ids, false, false).await?;
        migrate_relationships(source.as_ref(), dest.as_ref(), &skip_ids, false, false).await?;

        // Verify edge exists in destination
        let edges = dest.list_edges().await?;
        assert_eq!(edges.len(), 1, "Should have 1 edge in destination");
        assert_eq!(edges[0].from_id, mem1_id);
        assert_eq!(edges[0].to_id, mem2_id);
        assert_eq!(edges[0].rel_type, "Supports");

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_all_edge_types() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create memories
        let mem_ids: Vec<String> = (0..7).map(|i| format!("mem{}", i)).collect();
        for (i, id) in mem_ids.iter().enumerate() {
            source.add_memory(&voidm_core::models::AddMemoryRequest {
                content: format!("Memory {}", i),
                memory_type: "episodic".to_string(),
                tags: vec![],
                scope: None,
                id: Some(id.clone()),
            }).await?;
        }

        // Create edges of different types
        source.link_memories(&mem_ids[0], &mem_ids[1], "SUPPORTS", None).await?;
        source.link_memories(&mem_ids[1], &mem_ids[2], "CONTRADICTS", None).await?;
        source.link_memories(&mem_ids[2], &mem_ids[3], "DERIVED_FROM", None).await?;
        source.link_memories(&mem_ids[3], &mem_ids[4], "PART_OF", None).await?;
        source.link_memories(&mem_ids[4], &mem_ids[5], "PRECEDES", None).await?;
        source.link_memories(&mem_ids[5], &mem_ids[6], "RELATES_TO", None).await?;

        // Migrate
        let skip_ids = std::collections::HashSet::new();
        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &None, &skip_ids, false, false).await?;
        migrate_relationships(source.as_ref(), dest.as_ref(), &skip_ids, false, false).await?;

        // Verify all edges migrated with correct types
        let edges = dest.list_edges().await?;
        assert_eq!(edges.len(), 6, "Should have 6 edges");

        let types: std::collections::HashMap<_, _> = edges.iter().map(|e| (e.from_id.as_str(), e.rel_type.as_str())).collect();
        assert_eq!(types[mem_ids[0].as_str()], "Supports");
        assert_eq!(types[mem_ids[1].as_str()], "Contradicts");
        assert_eq!(types[mem_ids[2].as_str()], "DerivedFrom");
        assert_eq!(types[mem_ids[3].as_str()], "PartOf");
        assert_eq!(types[mem_ids[4].as_str()], "Precedes");
        assert_eq!(types[mem_ids[5].as_str()], "RelatesTo");

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_with_skip_ids() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create three memories
        let id1 = "keep-this";
        let id2 = "skip-this";
        let id3 = "keep-this-too";

        for id in &[id1, id2, id3] {
            source.add_memory(&voidm_core::models::AddMemoryRequest {
                content: format!("Memory {}", id),
                memory_type: "episodic".to_string(),
                tags: vec![],
                scope: None,
                id: Some(id.to_string()),
            }).await?;
        }

        // Migrate with skip list
        let mut skip_ids = std::collections::HashSet::new();
        skip_ids.insert(id2.to_string());

        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &None, &skip_ids, false, false).await?;

        // Verify only 2 memories in destination
        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 2, "Should have 2 memories (1 skipped)");
        assert!(dest_mems.iter().any(|m| m.id == id1));
        assert!(dest_mems.iter().any(|m| m.id == id3));
        assert!(!dest_mems.iter().any(|m| m.id == id2));

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_with_scope_filter() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create memories with different scopes
        source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "Project A memory".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec![],
            scope: Some("project/alpha".to_string()),
            id: None,
        }).await?;

        source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "Project B memory".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec![],
            scope: Some("project/beta".to_string()),
            id: None,
        }).await?;

        source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "Other memory".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec![],
            scope: Some("other".to_string()),
            id: None,
        }).await?;

        // Migrate with scope filter
        let skip_ids = std::collections::HashSet::new();
        let scope_filter = Some("project/alpha".to_string());

        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &scope_filter, &skip_ids, false, false).await?;

        // Verify only filtered scope migrated
        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 1, "Should have 1 memory (filtered by scope)");
        assert_eq!(dest_mems[0].content, "Project A memory");

        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_ontology_edges() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create memory and concept
        let mem_id = "mem1";
        let concept_id = "concept1";

        source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "Test memory".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec![],
            scope: None,
            id: Some(mem_id.to_string()),
        }).await?;

        source.add_concept("TestConcept", None, None, Some(concept_id)).await?;

        // Link them
        source.link_memory_to_concept(mem_id, concept_id).await?;

        // Migrate
        let skip_ids = std::collections::HashSet::new();
        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &None, &skip_ids, false, false).await?;
        migrate_concepts(source.as_ref(), dest.as_ref(), &None, false, false).await?;
        migrate_ontology_edges(source.as_ref(), dest.as_ref(), &skip_ids, false, false).await?;

        // Verify ontology edge exists
        let ontology_edges = dest.list_ontology_edges().await?;
        assert!(ontology_edges.len() > 0, "Should have at least 1 ontology edge");

        Ok(())
    }

    #[tokio::test]
    async fn test_roundtrip_migration_sqlite_to_sqlite() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create comprehensive test data in source
        let mem1_id = "mem1";
        let mem2_id = "mem2";
        let concept_id = "concept1";

        let m1 = source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "First memory".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec!["test".to_string()],
            scope: Some("test/scope".to_string()),
            id: Some(mem1_id.to_string()),
        }).await?;

        let m2 = source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "Second memory".to_string(),
            memory_type: "semantic".to_string(),
            tags: vec!["related".to_string()],
            scope: Some("test/scope".to_string()),
            id: Some(mem2_id.to_string()),
        }).await?;

        source.add_concept("TestConcept", Some("A test concept"), Some("test"), Some(concept_id)).await?;

        // Create relationships
        source.link_memories(mem1_id, mem2_id, "SUPPORTS", None).await?;
        source.link_memory_to_concept(mem1_id, concept_id).await?;

        // Full migration
        let skip_ids = std::collections::HashSet::new();
        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &None, &skip_ids, false, false).await?;
        migrate_concepts(source.as_ref(), dest.as_ref(), &None, false, false).await?;
        migrate_relationships(source.as_ref(), dest.as_ref(), &skip_ids, false, false).await?;
        migrate_ontology_edges(source.as_ref(), dest.as_ref(), &skip_ids, false, false).await?;

        // Verify all data
        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 2);
        assert_eq!(dest_mems[0].id, mem1_id);
        assert_eq!(dest_mems[1].id, mem2_id);

        let dest_concepts = dest.list_concepts().await?;
        assert_eq!(dest_concepts.len(), 1);
        assert_eq!(dest_concepts[0].id, concept_id);

        let dest_edges = dest.list_edges().await?;
        assert_eq!(dest_edges.len(), 1);
        assert_eq!(dest_edges[0].from_id, mem1_id);
        assert_eq!(dest_edges[0].to_id, mem2_id);

        let dest_ontology = dest.list_ontology_edges().await?;
        assert!(dest_ontology.len() > 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_dry_run_no_modifications() -> Result<()> {
        let source = create_test_db().await?;
        let dest = create_test_db().await?;

        // Create test data in source
        source.add_memory(&voidm_core::models::AddMemoryRequest {
            content: "Test".to_string(),
            memory_type: "episodic".to_string(),
            tags: vec![],
            scope: None,
            id: Some("test".to_string()),
        }).await?;

        // Run migration in dry-run mode
        let skip_ids = std::collections::HashSet::new();
        migrate_memories(source.as_ref(), dest.as_ref(), &voidm_core::Config::default(), &None, &skip_ids, true, false).await?;

        // Verify destination is still empty
        let dest_mems = dest.list_memories(Some(10)).await?;
        assert_eq!(dest_mems.len(), 0, "Dry run should not modify destination");

        Ok(())
    }
}
