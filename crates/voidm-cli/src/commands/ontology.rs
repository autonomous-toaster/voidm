use anyhow::Result;
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use voidm_core::ontology::{
    self, HierarchyDirection, NodeKind, OntologyRelType,
};
use voidm_core::Config;

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
    /// Enrich all unenriched concepts with NLI relation suggestions (downloads model on first use)
    Enrich(EnrichArgs),
    /// Benchmark NLI inference latency
    Benchmark,
    /// Extract named entities from text and propose them as concept candidates
    Extract(ExtractArgs),
    /// Batch-enrich all memories with NER entity extraction, auto-linking to existing concepts
    EnrichMemories(EnrichMemoriesArgs),
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
    /// Run NLI enrichment: suggest relations to existing concepts (downloads model ~180MB on first use)
    #[arg(long)]
    pub enrich: bool,
}

#[derive(Args)]
pub struct EnrichArgs {
    /// Max candidates per concept to score (default: 10)
    #[arg(long, default_value = "10")]
    pub top_k: usize,
}

#[derive(Args)]
pub struct ExtractArgs {
    /// Text to extract entities from
    pub text: String,
    /// Minimum confidence score to include (0.0–1.0, default: 0.7)
    #[arg(long, default_value = "0.7")]
    pub min_score: f32,
    /// Automatically add confirmed candidates as concepts (without this flag, only proposes)
    #[arg(long)]
    pub add: bool,
    /// Scope to assign to auto-added concepts
    #[arg(long)]
    pub scope: Option<String>,
}

#[derive(Args)]
pub struct EnrichMemoriesArgs {
    /// Only process memories with this scope prefix
    #[arg(long, short)]
    pub scope: Option<String>,
    /// Minimum NER confidence score (default: 0.7)
    #[arg(long, default_value = "0.7")]
    pub min_score: f32,
    /// Automatically create missing concepts (otherwise only links to existing ones)
    #[arg(long)]
    pub add: bool,
    /// Re-process memories already enriched (default: skip them)
    #[arg(long)]
    pub force: bool,
    /// Show what would be done without writing anything
    #[arg(long)]
    pub dry_run: bool,
    /// Max number of memories to process (default: all)
    #[arg(long, default_value = "0")]
    pub limit: usize,
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

pub async fn run(cmd: OntologyCommands, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    match cmd {
        OntologyCommands::Concept(sub) => run_concept(sub, pool, config, json).await,
        OntologyCommands::Link(args) => run_link(args, pool, json).await,
        OntologyCommands::Unlink(args) => run_unlink(args, pool, json).await,
        OntologyCommands::Edges(args) => run_edges(args, pool, json).await,
        OntologyCommands::Hierarchy(args) => run_hierarchy(args, pool, json).await,
        OntologyCommands::Instances(args) => run_instances(args, pool, json).await,
        OntologyCommands::Enrich(args) => run_enrich(args, pool, config, json).await,
        OntologyCommands::Benchmark => run_benchmark(json).await,
        OntologyCommands::Extract(args) => run_extract(args, pool, json).await,
        OntologyCommands::EnrichMemories(args) => run_enrich_memories(args, pool, json).await,
    }
}

// ─── Concept handlers ─────────────────────────────────────────────────────────

async fn run_concept(cmd: ConceptCommands, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    match cmd {
        ConceptCommands::Add(args) => {
            let concept = ontology::add_concept(
                pool,
                &args.name,
                args.description.as_deref(),
                args.scope.as_deref(),
            )
            .await?;

            // NLI enrichment if requested
            let suggestions = if args.enrich {
                run_enrichment_for_concept(&concept.id, &concept_text(&concept), pool, config, 10).await
            } else {
                vec![]
            };

            if json {
                let mut resp = serde_json::to_value(&concept)?;
                resp["suggested_relations"] = serde_json::to_value(&suggestions)?;
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                println!("Concept added: {} ({})", concept.name, &concept.id[..8]);
                if let Some(ref d) = concept.description { println!("  Description: {}", d); }
                if let Some(ref s) = concept.scope { println!("  Scope: {}", s); }
                if !suggestions.is_empty() {
                    println!("Suggested relations ({}):", suggestions.len());
                    for s in &suggestions {
                        println!("  [{:.2}] {} --[{}]--> {} \"{}\"",
                            s.confidence, &concept.id[..8], s.suggested_rel,
                            &s.candidate_id[..8.min(s.candidate_id.len())],
                            &s.candidate_text[..60.min(s.candidate_text.len())]);
                    }
                    println!("Use 'voidm ontology link' to confirm any of the above.");
                }
            }
        }
        ConceptCommands::Get(args) => {
            let concept = ontology::get_concept_with_instances(pool, &args.id).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&concept)?);
            } else {
                println!("[{}] {}", &concept.id[..8], concept.name);
                if let Some(ref d) = concept.description { println!("  {}", d); }
                if let Some(ref s) = concept.scope { println!("  scope: {}", s); }
                println!("  created: {}", concept.created_at);
                if !concept.superclasses.is_empty() {
                    println!("  IS_A: {}", concept.superclasses.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
                }
                if !concept.subclasses.is_empty() {
                    println!("  Subclasses: {}", concept.subclasses.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
                }
                if concept.instances.is_empty() {
                    println!("  Instances: none");
                } else {
                    println!("  Instances ({}):", concept.instances.len());
                    for inst in &concept.instances {
                        println!("    [{}] {}", &inst.memory_id[..8], inst.preview);
                    }
                }
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

// ─── NLI enrichment ──────────────────────────────────────────────────────────

/// Build a text representation for NLI scoring from a concept.
fn concept_text(c: &ontology::Concept) -> String {
    match &c.description {
        Some(d) => format!("{}: {}", c.name, d),
        None => c.name.clone(),
    }
}

/// Run NLI enrichment for a single concept against all other concepts.
/// Returns relation suggestions sorted by confidence.
async fn run_enrichment_for_concept(
    concept_id: &str,
    concept_text: &str,
    pool: &SqlitePool,
    config: &Config,
    top_k: usize,
) -> Vec<voidm_core::nli::RelationSuggestion> {
    // Ensure model is loaded
    if let Err(e) = voidm_core::nli::ensure_nli_model().await {
        eprintln!("Warning: NLI model load failed: {}. Skipping enrichment.", e);
        return vec![];
    }

    // Get all other concepts
    let candidates = match ontology::list_concepts(pool, None, 500).await {
        Ok(cs) => cs,
        Err(e) => {
            tracing::warn!("Failed to list concepts for enrichment: {}", e);
            return vec![];
        }
    };

    // Build candidate list: (id, text, similarity)
    // Use embedding similarity if available, else default to 0.5
    let mut scored_candidates: Vec<(String, String, f32)> = candidates
        .into_iter()
        .filter(|c| c.id != concept_id)
        .map(|c| {
            let text = concept_text_from(&c);
            (c.id, text, 0.5_f32) // similarity placeholder — real cosine would require embeddings
        })
        .collect();

    // If embeddings available, compute actual cosine similarity
    if config.embeddings.enabled {
        if let Ok(query_emb) = voidm_core::embeddings::embed_text(&config.embeddings.model, concept_text) {
            for (_id, text, sim) in &mut scored_candidates {
                if let Ok(emb) = voidm_core::embeddings::embed_text(&config.embeddings.model, text) {
                    *sim = cosine_similarity(&query_emb, &emb);
                }
            }
        }
    }

    // Sort by similarity, take top_k
    scored_candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored_candidates.truncate(top_k);

    voidm_core::nli::suggest_relations(concept_text, &scored_candidates)
}

fn concept_text_from(c: &ontology::Concept) -> String {
    match &c.description {
        Some(d) => format!("{}: {}", c.name, d),
        None => c.name.clone(),
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

async fn run_enrich(args: EnrichArgs, pool: &SqlitePool, config: &Config, json: bool) -> Result<()> {
    let concepts = ontology::list_concepts(pool, None, 1000).await?;
    if concepts.is_empty() {
        if json {
            println!("{}", serde_json::json!({ "enriched": 0, "message": "No concepts to enrich." }));
        } else {
            println!("No concepts to enrich.");
        }
        return Ok(());
    }

    println!("Enriching {} concept(s) with NLI relation suggestions …", concepts.len());

    let mut all_suggestions: Vec<serde_json::Value> = vec![];
    for concept in &concepts {
        let text = concept_text_from(concept);
        let suggestions = run_enrichment_for_concept(
            &concept.id, &text, pool, config, args.top_k,
        ).await;

        if !suggestions.is_empty() {
            if json {
                all_suggestions.push(serde_json::json!({
                    "concept_id": concept.id,
                    "concept_name": concept.name,
                    "suggestions": serde_json::to_value(&suggestions)?
                }));
            } else {
                println!("\n[{}] {}:", &concept.id[..8], concept.name);
                for s in &suggestions {
                    println!("  [{:.2}] --[{}]--> {} ({}) \"{}\"",
                        s.confidence, s.suggested_rel,
                        &s.candidate_id[..8.min(s.candidate_id.len())],
                        s.suggested_rel,
                        &s.candidate_text[..60.min(s.candidate_text.len())]);
                }
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&all_suggestions)?);
    } else {
        println!("\nDone. Use 'voidm ontology link' to confirm suggested relations.");
    }
    Ok(())
}

async fn run_benchmark(json: bool) -> Result<()> {
    println!("Loading NLI model …");
    voidm_core::nli::ensure_nli_model().await?;

    let avg_ms = voidm_core::nli::benchmark_latency(10)?;
    if json {
        println!("{}", serde_json::json!({ "avg_ms": avg_ms, "runs": 10 }));
    } else {
        println!("NLI inference latency: {:.1}ms avg (10 runs)", avg_ms);
        if avg_ms < 200.0 {
            println!("✓ Fast enough for synchronous enrichment on insert.");
        } else {
            println!("⚠ Latency > 200ms — recommend using --enrich flag explicitly.");
        }
    }
    Ok(())
}

async fn run_extract(args: ExtractArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    // Ensure model is loaded
    voidm_core::ner::ensure_ner_model().await?;

    // Extract entities
    let entities = voidm_core::ner::extract_entities(&args.text)?;

    // Filter by min_score
    let filtered: Vec<_> = entities.iter()
        .filter(|e| e.score >= args.min_score)
        .collect();

    if filtered.is_empty() {
        if json {
            println!("{}", serde_json::json!({ "candidates": [], "message": "No entities found above threshold." }));
        } else {
            println!("No entities found above score threshold {:.2}.", args.min_score);
            println!("Try lowering --min-score or providing more descriptive text.");
        }
        return Ok(());
    }

    // Check against existing concepts
    let entities_owned: Vec<voidm_core::ner::NamedEntity> = filtered.iter().map(|e| (*e).clone()).collect();
    let candidates = voidm_core::ner::entities_to_candidates(&entities_owned, pool).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&candidates)?);
        if !args.add { return Ok(()); }
    } else {
        println!("Extracted {} candidate(s):", candidates.len());
        for c in &candidates {
            let status = if c.already_exists {
                format!(" [exists: {}]", c.existing_id.as_deref().unwrap_or("?").get(..8).unwrap_or("?"))
            } else {
                String::new()
            };
            println!("  [{:.2}] {:5} {}{}", c.score, c.entity_type, c.name, status);
        }
    }

    // Auto-add if --add flag given
    if args.add {
        let new_candidates: Vec<_> = candidates.iter().filter(|c| !c.already_exists).collect();
        if new_candidates.is_empty() {
            if !json { println!("All candidates already exist as concepts."); }
            return Ok(());
        }

        if !json { println!("\nAdding {} new concept(s):", new_candidates.len()); }

        let mut added = Vec::new();
        for c in &new_candidates {
            match ontology::add_concept(pool, &c.name, None, args.scope.as_deref()).await {
                Ok(concept) => {
                    if !json {
                        println!("  ✓ {} [{}] ({})", concept.name, &concept.id[..8], c.entity_type);
                    }
                    added.push(concept);
                }
                Err(e) => {
                    if !json { eprintln!("  ✗ {}: {}", c.name, e); }
                }
            }
        }

        if json {
            println!("{}", serde_json::to_string_pretty(&added)?);
        } else {
            println!("\n{} concept(s) added. Use 'voidm ontology link' to build the hierarchy.", added.len());
        }
    } else if !json {
        println!("\nUse 'voidm ontology extract \"...\" --add' to automatically add new candidates.");
        println!("Or 'voidm ontology concept add \"<name>\"' to add individually.");
    }

    Ok(())
}

// ─── enrich-memories ──────────────────────────────────────────────────────────

async fn run_enrich_memories(
    args: EnrichMemoriesArgs,
    pool: &SqlitePool,
    json: bool,
) -> Result<()> {
    // Ensure NER model is loaded (downloads ~103MB on first use)
    if !json {
        if !voidm_core::ner::ner_model_downloaded() {
            eprintln!("Downloading NER model (~103MB, first use only) …");
        }
    }
    voidm_core::ner::ensure_ner_model().await?;

    let opts = voidm_core::ontology::EnrichMemoriesOpts {
        scope: args.scope.as_deref(),
        min_score: args.min_score,
        add: args.add,
        force: args.force,
        dry_run: args.dry_run,
        limit: args.limit,
    };

    let results = voidm_core::ontology::enrich_memories(pool, &opts).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    // Human-readable output
    let total = results.len();
    let skipped = results.iter().filter(|r| r.skipped).count();
    let processed = results.iter().filter(|r| !r.skipped).count();
    let total_links: usize = results.iter().map(|r| r.links_created).sum();
    let total_created: usize = results.iter().map(|r| r.concepts_created.len()).sum();

    if args.dry_run {
        println!("DRY RUN — no changes written.\n");
    }

    for (i, r) in results.iter().filter(|r| !r.skipped).enumerate() {
        let status = if r.entities_found == 0 {
            "no entities".to_string()
        } else {
            let mut parts = Vec::new();
            if !r.concepts_linked.is_empty() {
                parts.push(format!("linked: {}", r.concepts_linked.join(", ")));
            }
            if !r.concepts_created.is_empty() {
                parts.push(format!("created: {}", r.concepts_created.join(", ")));
            }
            if parts.is_empty() {
                format!("{} entities, 0 links (no matching concepts)", r.entities_found)
            } else {
                parts.join(" | ")
            }
        };
        println!(
            "[{}/{}] {} → {}",
            i + 1,
            processed,
            r.preview,
            status,
        );
    }

    if skipped > 0 {
        println!("\n{} already processed (use --force to re-run).", skipped);
    }

    println!(
        "\nDone: {}/{} memories processed, {} link(s) created, {} concept(s) created.",
        processed, total, total_links, total_created,
    );

    if !args.add && total_links < processed {
        println!("Tip: use --add to automatically create missing concepts.");
    }

    Ok(())
}
