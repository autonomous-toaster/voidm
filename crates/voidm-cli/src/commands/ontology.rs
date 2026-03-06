use anyhow::Result;
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use voidm_core::ontology::{
    self, HierarchyDirection, NodeKind, OntologyRelType,
};

// ─── Top-level subcommand tree ────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum OntologyCommands {
    /// Concept management
    #[command(subcommand)]
    Concept(ConceptCommands),
    /// Add a typed edge in the ontology graph
    Link(OntologyLinkArgs),
    /// Remove an ontology edge by id
    Unlink(OntologyUnlinkArgs),
    /// List all ontology edges for a node
    Edges(OntologyEdgesArgs),
    /// Show ancestors and descendants of a concept (IS_A hierarchy)
    Hierarchy(HierarchyArgs),
    /// List all instances of a concept, including subclasses
    Instances(InstancesArgs),
}

// ─── Concept subcommands ──────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum ConceptCommands {
    /// Add a new concept
    Add(ConceptAddArgs),
    /// Get a concept by ID
    Get(ConceptGetArgs),
    /// List concepts
    List(ConceptListArgs),
    /// Delete a concept
    Delete(ConceptDeleteArgs),
}

#[derive(Args)]
pub struct ConceptAddArgs {
    /// Concept name
    pub name: String,
    /// Optional description
    #[arg(long, short)]
    pub description: Option<String>,
    /// Optional scope (e.g. project/domain)
    #[arg(long, short)]
    pub scope: Option<String>,
}

#[derive(Args)]
pub struct ConceptGetArgs {
    /// Concept ID or short prefix (min 4 chars)
    pub id: String,
}

#[derive(Args)]
pub struct ConceptListArgs {
    /// Filter by scope prefix
    #[arg(long, short)]
    pub scope: Option<String>,
    /// Max results
    #[arg(long, default_value = "50")]
    pub limit: usize,
}

#[derive(Args)]
pub struct ConceptDeleteArgs {
    /// Concept ID or short prefix
    pub id: String,
}

// ─── Edge subcommands ─────────────────────────────────────────────────────────

#[derive(Args)]
pub struct OntologyLinkArgs {
    /// Source ID (concept or memory)
    pub from: String,
    /// Source kind: concept | memory
    #[arg(long, default_value = "concept")]
    pub from_kind: String,
    /// Relation type: IS_A, INSTANCE_OF, HAS_PROPERTY, or any existing EdgeType
    pub rel: String,
    /// Target ID (concept or memory)
    pub to: String,
    /// Target kind: concept | memory
    #[arg(long, default_value = "concept")]
    pub to_kind: String,
    /// Optional note
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Args)]
pub struct OntologyUnlinkArgs {
    /// Edge ID (integer from 'voidm ontology edges <id>')
    pub edge_id: i64,
}

#[derive(Args)]
pub struct OntologyEdgesArgs {
    /// Concept or memory ID
    pub id: String,
}

#[derive(Args)]
pub struct HierarchyArgs {
    /// Concept ID or short prefix
    pub id: String,
}

#[derive(Args)]
pub struct InstancesArgs {
    /// Concept ID or short prefix
    pub id: String,
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

pub async fn run(cmd: OntologyCommands, pool: &SqlitePool, json: bool) -> Result<()> {
    match cmd {
        OntologyCommands::Concept(sub) => run_concept(sub, pool, json).await,
        OntologyCommands::Link(args) => run_link(args, pool, json).await,
        OntologyCommands::Unlink(args) => run_unlink(args, pool, json).await,
        OntologyCommands::Edges(args) => run_edges(args, pool, json).await,
        OntologyCommands::Hierarchy(args) => run_hierarchy(args, pool, json).await,
        OntologyCommands::Instances(args) => run_instances(args, pool, json).await,
    }
}

// ─── Concept handlers ─────────────────────────────────────────────────────────

async fn run_concept(cmd: ConceptCommands, pool: &SqlitePool, json: bool) -> Result<()> {
    match cmd {
        ConceptCommands::Add(args) => {
            let concept = ontology::add_concept(
                pool,
                &args.name,
                args.description.as_deref(),
                args.scope.as_deref(),
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&concept)?);
            } else {
                println!("Concept added: {} ({})", concept.name, &concept.id[..8]);
                if let Some(ref d) = concept.description {
                    println!("  Description: {}", d);
                }
                if let Some(ref s) = concept.scope {
                    println!("  Scope: {}", s);
                }
            }
        }
        ConceptCommands::Get(args) => {
            let concept = ontology::get_concept(pool, &args.id).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&concept)?);
            } else {
                println!("[{}] {}", &concept.id[..8], concept.name);
                if let Some(ref d) = concept.description {
                    println!("  {}", d);
                }
                if let Some(ref s) = concept.scope {
                    println!("  scope: {}", s);
                }
                println!("  created: {}", concept.created_at);
            }
        }
        ConceptCommands::List(args) => {
            let concepts = ontology::list_concepts(pool, args.scope.as_deref(), args.limit).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&concepts)?);
            } else {
                if concepts.is_empty() {
                    println!("No concepts found. Use 'voidm ontology concept add <name>' to create one.");
                } else {
                    for c in &concepts {
                        let scope_str = c.scope.as_deref().map(|s| format!(" ({})", s)).unwrap_or_default();
                        let desc_str = c.description.as_deref()
                            .map(|d| if d.len() > 60 { format!(" — {}…", &d[..60]) } else { format!(" — {}", d) })
                            .unwrap_or_default();
                        println!("[{}]{} {}{}", &c.id[..8], scope_str, c.name, desc_str);
                    }
                    println!("{} concept(s)", concepts.len());
                }
            }
        }
        ConceptCommands::Delete(args) => {
            let deleted = ontology::delete_concept(pool, &args.id).await?;
            if json {
                println!("{}", serde_json::json!({ "deleted": deleted, "id": args.id }));
            } else if deleted {
                println!("Concept '{}' deleted.", args.id);
            } else {
                eprintln!("Concept '{}' not found.", args.id);
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

// ─── Edge handlers ────────────────────────────────────────────────────────────

async fn run_link(args: OntologyLinkArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let from_kind: NodeKind = args.from_kind.parse()?;
    let to_kind: NodeKind = args.to_kind.parse()?;
    let rel: OntologyRelType = args.rel.parse()?;

    // Resolve short IDs
    let from_id = resolve_node_id(pool, &args.from, &from_kind).await?;
    let to_id = resolve_node_id(pool, &args.to, &to_kind).await?;

    let edge = ontology::add_ontology_edge(
        pool,
        &from_id,
        from_kind,
        &rel,
        &to_id,
        to_kind,
        args.note.as_deref(),
    )
    .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&edge)?);
    } else {
        println!(
            "Linked: {} ({}) --[{}]--> {} ({})",
            &edge.from_id[..8], edge.from_type, edge.rel_type, &edge.to_id[..8], edge.to_type
        );
    }
    Ok(())
}

async fn run_unlink(args: OntologyUnlinkArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let deleted = ontology::delete_ontology_edge(pool, args.edge_id).await?;
    if json {
        println!("{}", serde_json::json!({ "deleted": deleted, "edge_id": args.edge_id }));
    } else if deleted {
        println!("Ontology edge {} removed.", args.edge_id);
    } else {
        eprintln!("Edge {} not found.", args.edge_id);
        std::process::exit(1);
    }
    Ok(())
}

async fn run_edges(args: OntologyEdgesArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    // Try to resolve as concept first, then as memory
    let full_id = match ontology::resolve_concept_id(pool, &args.id).await {
        Ok(id) => id,
        Err(_) => voidm_core::resolve_id(pool, &args.id).await?,
    };
    let edges = ontology::list_ontology_edges(pool, &full_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&edges)?);
    } else {
        if edges.is_empty() {
            println!("No ontology edges for '{}'.", args.id);
        } else {
            for e in &edges {
                println!(
                    "[{}] {} ({}) --[{}]--> {} ({})",
                    e.id,
                    &e.from_id[..8.min(e.from_id.len())],
                    e.from_type,
                    e.rel_type,
                    &e.to_id[..8.min(e.to_id.len())],
                    e.to_type,
                );
            }
            println!("{} edge(s)", edges.len());
        }
    }
    Ok(())
}

// ─── Hierarchy handler ────────────────────────────────────────────────────────

async fn run_hierarchy(args: HierarchyArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let concept = ontology::get_concept(pool, &args.id).await?;
    let nodes = ontology::concept_hierarchy(pool, &concept.id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&nodes)?);
        return Ok(());
    }

    let ancestors: Vec<_> = nodes.iter().filter(|n| matches!(n.direction, HierarchyDirection::Ancestor)).collect();
    let descendants: Vec<_> = nodes.iter().filter(|n| matches!(n.direction, HierarchyDirection::Descendant)).collect();

    if ancestors.is_empty() && descendants.is_empty() {
        println!("'{}' has no IS_A connections yet.", concept.name);
        println!("Use 'voidm ontology link <id> IS_A <parent-id>' to build the hierarchy.");
        return Ok(());
    }

    if !ancestors.is_empty() {
        println!("Ancestors (IS_A chain upward):");
        for n in &ancestors {
            println!("  {:indent$}{} [{}]", "", n.name, &n.id[..8], indent = (n.depth as usize - 1) * 2);
        }
    }

    println!("  → {} (self)", concept.name);

    if !descendants.is_empty() {
        println!("Descendants (subclasses):");
        for n in &descendants {
            println!("  {:indent$}{} [{}]", "", n.name, &n.id[..8], indent = (n.depth as usize - 1) * 2);
        }
    }

    Ok(())
}

// ─── Instances handler ────────────────────────────────────────────────────────

async fn run_instances(args: InstancesArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let concept = ontology::get_concept(pool, &args.id).await?;
    let instances = ontology::concept_instances(pool, &concept.id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&instances)?);
        return Ok(());
    }

    if instances.is_empty() {
        println!("No instances of '{}' (including subclasses) found.", concept.name);
        println!("Use 'voidm ontology link <id> --from-kind memory INSTANCE_OF {}' to link a memory.", &concept.id[..8]);
    } else {
        println!("Instances of '{}' (including subclasses):", concept.name);
        for inst in &instances {
            let via = if inst.concept_id != concept.id {
                format!(" (via subclass {})", &inst.concept_id[..8])
            } else {
                String::new()
            };
            println!(
                "  [{}] {} {}{}",
                &inst.instance_id[..8.min(inst.instance_id.len())],
                inst.instance_kind,
                inst.note.as_deref().map(|n| format!("— {}", n)).unwrap_or_default(),
                via
            );
        }
        println!("{} instance(s)", instances.len());
    }
    Ok(())
}

// ─── ID resolution helpers ────────────────────────────────────────────────────

async fn resolve_node_id(pool: &SqlitePool, id: &str, kind: &NodeKind) -> Result<String> {
    match kind {
        NodeKind::Concept => ontology::resolve_concept_id(pool, id).await,
        NodeKind::Memory => voidm_core::resolve_id(pool, id).await,
    }
}
