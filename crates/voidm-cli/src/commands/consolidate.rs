//! Consolidate command: unified memory cleanup & concept management.
//!
//! Runs 4 phases sequentially:
//! 1. Memory deduplication (cosine similarity >= threshold, same type)
//! 2. Entity extraction & concept creation (NER-based)
//! 3. Concept deduplication (same name/scope)
//! 4. NLI-based conflict detection (CONTRADICTS edges)

use anyhow::{Context, Result};
use clap::Args;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use voidm_core::Config;
use std::time::Instant;

#[derive(Args, Clone)]
pub struct ConsolidateArgs {
    /// Filter by scope prefix (optional)
    #[arg(long)]
    pub scope: Option<String>,

    /// Preview changes without applying
    #[arg(long)]
    pub dry_run: bool,

    /// Memory merge threshold (cosine similarity)
    #[arg(long, default_value = "0.98")]
    pub similarity_threshold: f32,

    /// Show per-item details
    #[arg(long)]
    pub verbose: bool,

    /// Skip confirmation prompt
    #[arg(long)]
    pub force: bool,
}

// ─── Phase Results Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConsolidateResults {
    pub phase_1_memory_dedup: Phase1Results,
    pub phase_2_entity_extraction: Phase2Results,
    pub phase_3_concept_dedup: Phase3Results,
    pub phase_4_nli_conflicts: Phase4Results,
    pub warnings: Vec<String>,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Phase1Results {
    pub memories_checked: usize,
    pub merged_pairs: usize,
    pub skipped_type_mismatch: usize,
    pub merged_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Phase2Results {
    pub memories_processed: usize,
    pub entities_extracted: usize,
    pub new_concepts: usize,
    pub existing_concepts_reused: usize,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Phase3Results {
    pub concepts_checked: usize,
    pub merge_groups: usize,
    pub concepts_deleted: usize,
    pub instances_remapped: usize,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Phase4Results {
    pub contradicts_edges_examined: usize,
    pub confirmed_contradictions: usize,
    pub flagged_for_review: usize,
}

// ─── Main Entry Point ──────────────────────────────────────────────────────

pub async fn run(
    args: ConsolidateArgs,
    _db: &std::sync::Arc<dyn voidm_db_trait::Database>,
    pool: &SqlitePool,
    _config: &Config,
    json: bool,
) -> Result<()> {
    let start = Instant::now();
    let mut results = ConsolidateResults {
        phase_1_memory_dedup: Phase1Results::default(),
        phase_2_entity_extraction: Phase2Results::default(),
        phase_3_concept_dedup: Phase3Results::default(),
        phase_4_nli_conflicts: Phase4Results::default(),
        warnings: Vec::new(),
        duration_ms: 0,
    };

    // Show what we're about to do
    if !args.dry_run && !args.force {
        eprint!(
            "About to consolidate memories and concepts{}\nContinue? [y/N] ",
            args.scope
                .as_ref()
                .map(|s| format!(" in scope '{}'", s))
                .unwrap_or_default()
        );
        use std::io::{self, Write};
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Phase 1: Memory Deduplication
    if !json {
        eprintln!("\n📋 Phase 1: Memory Deduplication...");
    }
    phase_1_memory_dedup(&args, pool, &mut results).await?;

    // Phase 2: Entity Extraction & Concept Creation
    if !json {
        eprintln!("📋 Phase 2: Entity Extraction...");
    }
    phase_2_entity_extraction(&args, pool, &mut results).await?;

    // Phase 3: Concept Deduplication
    if !json {
        eprintln!("📋 Phase 3: Concept Deduplication...");
    }
    phase_3_concept_dedup(&args, pool, &mut results).await?;

    // Phase 4: NLI-Based Conflict Detection
    if !json {
        eprintln!("📋 Phase 4: NLI-Based Conflict Detection...");
    }
    phase_4_nli_conflicts(&args, pool, &mut results).await?;

    results.duration_ms = start.elapsed().as_millis();

    // Output results
    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        print_human_output(&results, args.verbose);
    }

    Ok(())
}

// ─── Phase 1: Memory Deduplication ────────────────────────────────────────

async fn phase_1_memory_dedup(
    args: &ConsolidateArgs,
    pool: &SqlitePool,
    results: &mut ConsolidateResults,
) -> Result<()> {
    // Load all memories with their type and embedding
    let memories: Vec<(String, String, String, String)> = if let Some(scope) = &args.scope {
        sqlx::query_as(
            "SELECT m.id, m.type, m.content, m.created_at
             FROM memories m
             WHERE m.id IN (SELECT memory_id FROM memory_scopes WHERE scope LIKE ? || '%')
             ORDER BY m.created_at DESC"
        )
        .bind(scope)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, type, content, created_at FROM memories ORDER BY created_at DESC"
        )
        .fetch_all(pool)
        .await?
    };

    results.phase_1_memory_dedup.memories_checked = memories.len();

    if memories.is_empty() {
        if args.verbose && !args.dry_run {
            eprintln!("  No memories to deduplicate.");
        }
        return Ok(());
    }

    // Get embeddings for all memories
    let embedding_map: HashMap<String, Vec<f32>> = sqlx::query_as(
        "SELECT memory_id, embedding FROM vec_memories WHERE memory_id IN (SELECT id FROM memories)"
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, emb): (String, Vec<u8>)| {
        // Deserialize embedding from f32 le_bytes
        let emb_f32: Vec<f32> = emb.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        (id, emb_f32)
    })
    .collect();

    // Find similar pairs (cosine similarity)
    let mut merged_ids = HashSet::new();
    let mut pairs_to_merge: Vec<(String, String)> = Vec::new();

    for i in 0..memories.len() {
        if merged_ids.contains(&memories[i].0) {
            continue;
        }

        let (id1, type1, _, _) = &memories[i];
        let emb1 = match embedding_map.get(id1) {
            Some(e) => e,
            None => continue,
        };

        for j in (i + 1)..memories.len() {
            if merged_ids.contains(&memories[j].0) {
                continue;
            }

            let (id2, type2, _, _) = &memories[j];

            // Check type match
            if type1 != type2 {
                results.phase_1_memory_dedup.skipped_type_mismatch += 1;
                results.warnings.push(format!(
                    "Phase 1: Skipped merge (type mismatch): {} ({}) vs {} ({})",
                    id1, type1, id2, type2
                ));
                continue;
            }

            let emb2 = match embedding_map.get(id2) {
                Some(e) => e,
                None => continue,
            };

            // Compute cosine similarity
            let similarity = voidm_core::fast_vector::cosine_similarity(emb1, emb2);

            if similarity >= args.similarity_threshold {
                // Mark id2 (older) to be merged into id1 (newer, since sorted DESC)
                pairs_to_merge.push((id2.clone(), id1.clone()));
                merged_ids.insert(id2.clone());

                if args.verbose {
                    eprintln!(
                        "  Merged {} + {} → {} ({:.3} similar, type: {})",
                        id2, id1, id1, similarity, type1
                    );
                }

                break; // id2 is now merged, move to next i
            }
        }
    }

    results.phase_1_memory_dedup.merged_pairs = pairs_to_merge.len();
    results.phase_1_memory_dedup.merged_ids = pairs_to_merge
        .iter()
        .map(|(_, target)| target.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // Apply merges if not dry-run
    if !args.dry_run && !pairs_to_merge.is_empty() {
        for (source_id, target_id) in pairs_to_merge {
            // Get source and target metadata
            let (source_tags, source_importance): (String, i32) = sqlx::query_as(
                "SELECT tags, importance FROM memories WHERE id = ?"
            )
            .bind(&source_id)
            .fetch_one(pool)
            .await?;

            let (target_tags, target_importance): (String, i32) = sqlx::query_as(
                "SELECT tags, importance FROM memories WHERE id = ?"
            )
            .bind(&target_id)
            .fetch_one(pool)
            .await?;

            // Parse tags
            let source_tags_vec: Vec<String> = serde_json::from_str(&source_tags).unwrap_or_default();
            let target_tags_vec: Vec<String> = serde_json::from_str(&target_tags).unwrap_or_default();

            // Merge tags (union + deduplicate)
            let mut merged_tags = target_tags_vec;
            for tag in source_tags_vec {
                let normalized = tag.trim().to_lowercase();
                if !merged_tags.iter().any(|t| t.trim().to_lowercase() == normalized) {
                    merged_tags.push(tag);
                }
            }

            let merged_tags_json = serde_json::to_string(&merged_tags)?;

            // Max importance
            let merged_importance = target_importance.max(source_importance);

            // Update target with merged metadata
            sqlx::query(
                "UPDATE memories SET tags = ?, importance = ? WHERE id = ?"
            )
            .bind(&merged_tags_json)
            .bind(merged_importance)
            .bind(&target_id)
            .execute(pool)
            .await?;

            // Get source graph node ID and remap edges
            let source_node_id: Option<i64> =
                sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
                    .bind(&source_id)
                    .fetch_optional(pool)
                    .await?;

            if let Some(source_node) = source_node_id {
                // Remap edges: source_node → target_node
                let target_node_id: i64 = sqlx::query_scalar(
                    "SELECT id FROM graph_nodes WHERE memory_id = ?"
                )
                .bind(&target_id)
                .fetch_one(pool)
                .await?;

                // Remap outgoing edges
                sqlx::query("UPDATE graph_edges SET from_node = ? WHERE from_node = ?")
                    .bind(target_node_id)
                    .bind(source_node)
                    .execute(pool)
                    .await?;

                // Remap incoming edges
                sqlx::query("UPDATE graph_edges SET to_node = ? WHERE to_node = ?")
                    .bind(target_node_id)
                    .bind(source_node)
                    .execute(pool)
                    .await?;

                // Delete source graph node
                sqlx::query("DELETE FROM graph_nodes WHERE id = ?")
                    .bind(source_node)
                    .execute(pool)
                    .await?;
            }

            // Delete source memory
            sqlx::query("DELETE FROM memories WHERE id = ?")
                .bind(&source_id)
                .execute(pool)
                .await?;

            // Clean up embeddings
            sqlx::query("DELETE FROM vec_memories WHERE memory_id = ?")
                .bind(&source_id)
                .execute(pool)
                .await?;
        }
    }

    Ok(())
}

// ─── Phase 2: Entity Extraction & Concept Creation ──────────────────────────

async fn phase_2_entity_extraction(
    args: &ConsolidateArgs,
    pool: &SqlitePool,
    results: &mut ConsolidateResults,
) -> Result<()> {
    // Load all memories (after phase 1 merges)
    let memories: Vec<(String, String)> = if let Some(scope) = &args.scope {
        sqlx::query_as(
            "SELECT m.id, m.content
             FROM memories m
             WHERE m.id IN (SELECT memory_id FROM memory_scopes WHERE scope LIKE ? || '%')"
        )
        .bind(scope)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as("SELECT id, content FROM memories")
            .fetch_all(pool)
            .await?
    };

    results.phase_2_entity_extraction.memories_processed = memories.len();

    // Ensure NER model is loaded
    voidm_ner::ensure_ner_model()
        .await
        .context("Failed to load NER model")?;

    for (mem_id, content) in memories {
        // Extract entities
        let entities = match voidm_ner::extract_entities(&content) {
            Ok(ents) => ents,
            Err(e) => {
                results.warnings.push(format!(
                    "Phase 2: Entity extraction failed for {}: {}",
                    mem_id, e
                ));
                continue;
            }
        };

        results.phase_2_entity_extraction.entities_extracted += entities.len();

        // Convert to candidates and check existence
        let candidates = voidm_ner::entities_to_candidates(&entities, pool)
            .await
            .context("Failed to convert entities to candidates")?;

        for candidate in candidates {
            if candidate.already_exists {
                results.phase_2_entity_extraction.existing_concepts_reused += 1;

                // Link memory to existing concept
                if !args.dry_run {
                    if let Some(concept_id) = candidate.existing_id {
                        // Add INSTANCE_OF edge
                        link_memory_to_concept(pool, &mem_id, &concept_id).await?;
                    }
                }
            } else {
                results.phase_2_entity_extraction.new_concepts += 1;

                // Create new concept
                if !args.dry_run {
                    let concept_id = create_concept(pool, &candidate).await?;
                    // Link memory to concept
                    link_memory_to_concept(pool, &mem_id, &concept_id).await?;

                    if args.verbose {
                        eprintln!(
                            "  Extracted concept \"{}\" ({}, {:.2}) from {}",
                            candidate.name, candidate.entity_type, candidate.score, mem_id
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

// ─── Phase 3: Concept Deduplication ───────────────────────────────────────

async fn phase_3_concept_dedup(
    args: &ConsolidateArgs,
    pool: &SqlitePool,
    results: &mut ConsolidateResults,
) -> Result<()> {
    // Load all concepts with their scope
    let concepts: Vec<(String, String, String)> = if let Some(scope) = &args.scope {
        sqlx::query_as(
            "SELECT id, name, scope FROM ontology_concepts WHERE scope LIKE ? || '%'"
        )
        .bind(scope)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as("SELECT id, name, scope FROM ontology_concepts")
            .fetch_all(pool)
            .await?
    };

    results.phase_3_concept_dedup.concepts_checked = concepts.len();

    // Group by (name_normalized, scope)
    let mut groups: HashMap<(String, String), Vec<String>> = HashMap::new();
    for (id, name, scope) in concepts {
        let key = (name.to_lowercase(), scope);
        groups.entry(key).or_default().push(id);
    }

    // Find duplicates and merge
    let mut duplicates_to_merge: Vec<(Vec<String>, String)> = Vec::new();
    for ((_name_key, _scope), mut ids) in groups {
        if ids.len() > 1 {
            // Sort by creation timestamp to keep oldest
            ids.sort_by_key(|id| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let created_at: Option<String> =
                        sqlx::query_scalar("SELECT created_at FROM ontology_concepts WHERE id = ?")
                            .bind(id)
                            .fetch_optional(pool)
                            .await
                            .unwrap_or(None);
                    created_at.unwrap_or_default()
                })
            });

            let keeper = ids[0].clone();
            let to_delete = ids[1..].to_vec();
            duplicates_to_merge.push((to_delete, keeper));
        }
    }

    results.phase_3_concept_dedup.merge_groups = duplicates_to_merge.len();
    results.phase_3_concept_dedup.concepts_deleted =
        duplicates_to_merge.iter().map(|(d, _)| d.len()).sum();

    // Apply merges if not dry-run
    if !args.dry_run && !duplicates_to_merge.is_empty() {
        for (to_delete, keeper) in duplicates_to_merge {
            // Merge tags from all to_delete concepts into keeper
            let keeper_tags: String =
                sqlx::query_scalar("SELECT tags FROM ontology_concepts WHERE id = ?")
                    .bind(&keeper)
                    .fetch_one(pool)
                    .await?;

            let mut merged_tags: Vec<String> =
                serde_json::from_str(&keeper_tags).unwrap_or_default();

            for concept_id in &to_delete {
                let tags: String = sqlx::query_scalar(
                    "SELECT tags FROM ontology_concepts WHERE id = ?"
                )
                .bind(concept_id)
                .fetch_one(pool)
                .await?;

                let tags_vec: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
                for tag in tags_vec {
                    let normalized = tag.trim().to_lowercase();
                    if !merged_tags.iter().any(|t| t.trim().to_lowercase() == normalized) {
                        merged_tags.push(tag);
                    }
                }
            }

            let merged_tags_json = serde_json::to_string(&merged_tags)?;
            sqlx::query("UPDATE ontology_concepts SET tags = ? WHERE id = ?")
                .bind(&merged_tags_json)
                .bind(&keeper)
                .execute(pool)
                .await?;

            // Remap INSTANCE_OF edges from deleted concepts to keeper
            for concept_id in &to_delete {
                sqlx::query(
                    "UPDATE graph_edges SET from_node = (SELECT id FROM graph_nodes WHERE concept_id = ?)
                     WHERE from_node = (SELECT id FROM graph_nodes WHERE concept_id = ?)"
                )
                .bind(&keeper)
                .bind(concept_id)
                .execute(pool)
                .await?;

                // Delete concept node and edges
                let concept_node_id: Option<i64> =
                    sqlx::query_scalar("SELECT id FROM graph_nodes WHERE concept_id = ?")
                        .bind(concept_id)
                        .fetch_optional(pool)
                        .await?;

                if let Some(node_id) = concept_node_id {
                    sqlx::query("DELETE FROM graph_edges WHERE from_node = ? OR to_node = ?")
                        .bind(node_id)
                        .bind(node_id)
                        .execute(pool)
                        .await?;
                    sqlx::query("DELETE FROM graph_nodes WHERE id = ?")
                        .bind(node_id)
                        .execute(pool)
                        .await?;
                }

                // Delete concept
                sqlx::query("DELETE FROM ontology_concepts WHERE id = ?")
                    .bind(concept_id)
                    .execute(pool)
                    .await?;

                results.phase_3_concept_dedup.instances_remapped += 1;
            }

            if args.verbose {
                eprintln!(
                    "  Merged {} concepts into {} (kept oldest)",
                    to_delete.len(),
                    keeper
                );
            }
        }
    }

    Ok(())
}

// ─── Phase 4: NLI-Based Conflict Detection ────────────────────────────────

async fn phase_4_nli_conflicts(
    args: &ConsolidateArgs,
    pool: &SqlitePool,
    results: &mut ConsolidateResults,
) -> Result<()> {
    // Query all CONTRADICTS edges (ontology_edges table)
    let edges: Vec<(String, String)> = sqlx::query_as(
        "SELECT oc1.description, oc2.description
         FROM ontology_edges e
         JOIN ontology_concepts oc1 ON e.from_id = oc1.id
         JOIN ontology_concepts oc2 ON e.to_id = oc2.id
         WHERE e.rel_type = 'CONTRADICTS'
           AND e.from_type = 'concept'
           AND e.to_type = 'concept'"
    )
    .fetch_all(pool)
    .await?;

    results.phase_4_nli_conflicts.contradicts_edges_examined = edges.len();

    if edges.is_empty() {
        return Ok(());
    }

    // Ensure NLI model is loaded
    voidm_nli::ensure_nli_model()
        .await
        .context("Failed to load NLI model")?;

    let mut confirmed = 0;

    for (text1, text2) in edges {
        // Run NLI classifier
        let scores = voidm_nli::classify(&text1, &text2)
            .map_err(|e| {
                results.warnings.push(format!(
                    "Phase 4: NLI classification failed: {}",
                    e
                ));
                e
            })?;

        if scores.contradiction >= 0.7 {
            confirmed += 1;

            if args.verbose {
                eprintln!(
                    "  CONTRADICTS: (entailment: {:.2}, contradiction: {:.2})",
                    scores.entailment, scores.contradiction
                );
            }
        }
    }

    results.phase_4_nli_conflicts.confirmed_contradictions = confirmed;
    results.phase_4_nli_conflicts.flagged_for_review = confirmed;

    Ok(())
}

// ─── Helper Functions ──────────────────────────────────────────────────────

async fn link_memory_to_concept(pool: &SqlitePool, mem_id: &str, concept_id: &str) -> Result<()> {
    let mem_node_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM graph_nodes WHERE memory_id = ?")
            .bind(mem_id)
            .fetch_optional(pool)
            .await?;

    let concept_node_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM graph_nodes WHERE concept_id = ?")
            .bind(concept_id)
            .fetch_optional(pool)
            .await?;

    if let (Some(mem_node), Some(concept_node)) = (mem_node_id, concept_node_id) {
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges (from_node, to_node, rel_type) VALUES (?, ?, 'INSTANCE_OF')"
        )
        .bind(mem_node)
        .bind(concept_node)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn create_concept(pool: &SqlitePool, candidate: &voidm_ner::ConceptCandidate) -> Result<String> {
    let concept_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    sqlx::query(
        "INSERT INTO ontology_concepts (id, name, scope, description, tags, created_at)
         VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&concept_id)
    .bind(&candidate.name)
    .bind("") // scope left empty
    .bind(format!("Extracted entity type: {}", candidate.entity_type))
    .bind("[]")
    .bind(&now)
    .execute(pool)
    .await?;

    // Create graph node for concept
    sqlx::query("INSERT INTO graph_nodes (concept_id) VALUES (?)")
        .bind(&concept_id)
        .execute(pool)
        .await?;

    Ok(concept_id)
}

// ─── Output Formatting ────────────────────────────────────────────────────

fn print_human_output(results: &ConsolidateResults, _verbose: bool) {
    println!("\n✅ Consolidation Complete\n");

    println!("Phase 1: Memory Deduplication");
    println!("  Checked: {} memories", results.phase_1_memory_dedup.memories_checked);
    println!("  Merged: {} pairs", results.phase_1_memory_dedup.merged_pairs);
    if results.phase_1_memory_dedup.skipped_type_mismatch > 0 {
        println!(
            "  Skipped (type mismatch): {}",
            results.phase_1_memory_dedup.skipped_type_mismatch
        );
    }

    println!("\nPhase 2: Entity Extraction");
    println!(
        "  Memories processed: {}",
        results.phase_2_entity_extraction.memories_processed
    );
    println!(
        "  Entities extracted: {}",
        results.phase_2_entity_extraction.entities_extracted
    );
    println!(
        "  New concepts created: {}",
        results.phase_2_entity_extraction.new_concepts
    );
    println!(
        "  Existing concepts reused: {}",
        results.phase_2_entity_extraction.existing_concepts_reused
    );

    println!("\nPhase 3: Concept Deduplication");
    println!(
        "  Concepts checked: {}",
        results.phase_3_concept_dedup.concepts_checked
    );
    println!(
        "  Merge groups found: {}",
        results.phase_3_concept_dedup.merge_groups
    );
    println!(
        "  Concepts deleted: {}",
        results.phase_3_concept_dedup.concepts_deleted
    );
    println!(
        "  Instances remapped: {}",
        results.phase_3_concept_dedup.instances_remapped
    );

    println!("\nPhase 4: NLI-Based Conflict Detection");
    println!(
        "  CONTRADICTS edges examined: {}",
        results.phase_4_nli_conflicts.contradicts_edges_examined
    );
    println!(
        "  Confirmed contradictions: {}",
        results.phase_4_nli_conflicts.confirmed_contradictions
    );

    if !results.warnings.is_empty() {
        println!("\n⚠️  Warnings ({}):", results.warnings.len());
        for warning in &results.warnings {
            println!("  {}", warning);
        }
    }

    println!("\n⏱️  Duration: {}ms", results.duration_ms);
}
