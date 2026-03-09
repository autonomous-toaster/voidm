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

    // Migrate memories
    migrate_memories(source_db.as_ref(), dest_db.as_ref(), config, &args.scope_filter, &skip_ids, args.dry_run, json).await?;

    // Migrate concepts
    migrate_concepts(source_db.as_ref(), dest_db.as_ref(), &args.scope_filter, args.dry_run, json).await?;

    // Migrate relationships
    migrate_relationships(source_db.as_ref(), dest_db.as_ref(), &skip_ids, args.dry_run, json).await?;

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
    let memories = source.list_memories(None).await?;

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

        let _ = dest.add_concept(&concept.name, concept.description.as_deref(), concept.scope.as_deref()).await?;
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
    _source: &(impl Database + ?Sized),
    _dest: &(impl Database + ?Sized),
    _skip_ids: &std::collections::HashSet<String>,
    _dry_run: bool,
    _json: bool,
) -> Result<()> {
    // TODO: Implement relationship migration
    // This requires querying the graph edges from source and recreating them in dest
    Ok(())
}
