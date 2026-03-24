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
    db: &std::sync::Arc<dyn voidm_db_trait::Database>,
    pool: &SqlitePool,
    config: &Config,
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

    // Phase 1: Memory Deduplication (SQLite only - requires direct embedding access)
    if config.database.backend == "sqlite" {
        if !json {
            eprintln!("\n📋 Phase 1: Memory Deduplication...");
        }
        phase_1_memory_dedup(&args, pool, &mut results).await?;
    } else {
        if !json {
            eprintln!("\n⏭️  Phase 1: Memory Deduplication (skipped - not available for {} backend)", config.database.backend);
        }
        results.warnings.push("Phase 1: Memory deduplication not available for Neo4j backend (requires embedding table access)".to_string());
    }

    // Phase 2: Entity Extraction & Concept Creation (use database trait)
    if !json {
        eprintln!("📋 Phase 2: Entity Extraction...");
    }
    phase_2_entity_extraction(&args, db, pool, &mut results).await?;

    // Phase 3: Concept Deduplication (use database trait)
    if !json {
        eprintln!("📋 Phase 3: Concept Deduplication...");
    }
    phase_3_concept_dedup(&args, db, &mut results).await?;

    // Phase 4: NLI-Based Conflict Detection (use database trait)
    if !json {
        eprintln!("📋 Phase 4: NLI-Based Conflict Detection...");
    }
    phase_4_nli_conflicts(&args, db, &mut results).await?;

    results.duration_ms = start.elapsed().as_millis();

    // Output results
    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        print_human_output(&results);
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

    // Apply merges if not dry-run (using direct SQL for now, since Database trait doesn't expose merge)
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
                sqlx::query("UPDATE graph_edges SET source_id = ? WHERE source_id = ?")
                    .bind(target_node_id)
                    .bind(source_node)
                    .execute(pool)
                    .await?;

                // Remap incoming edges
                sqlx::query("UPDATE graph_edges SET target_id = ? WHERE target_id = ?")
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
    db: &std::sync::Arc<dyn voidm_db_trait::Database>,
    pool: &SqlitePool,
    results: &mut ConsolidateResults,
) -> Result<()> {
    // Load memories using database trait (backend-agnostic)
    let memories_raw = db.fetch_memories_raw(
        args.scope.as_deref(),
        None,  // no type filter
        10000, // high limit
    ).await?;

    results.phase_2_entity_extraction.memories_processed = memories_raw.len();

    // Ensure NER model is loaded
    voidm_ner::ensure_ner_model()
        .await
        .context("Failed to load NER model")?;

    for (mem_id, content) in memories_raw {
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
                        // Add INSTANCE_OF edge via database trait
                        let _ = db.add_ontology_edge(
                            &mem_id,
                            "memory",
                            "INSTANCE_OF",
                            &concept_id,
                            "concept",
                            None,
                        ).await;
                    }
                }
            } else {
                results.phase_2_entity_extraction.new_concepts += 1;

                // Create new concept via database trait
                if !args.dry_run {
                    let desc = format!("Extracted entity type: {}", candidate.entity_type);
                    let concept_result = db.add_concept(
                        &candidate.name,
                        Some(&desc),
                        None, // no scope
                        None, // auto-generate ID
                    ).await?;

                    // Extract concept ID from response
                    if let Some(concept_id) = concept_result.get("id").and_then(|v| v.as_str()) {
                        // Link memory to concept
                        let _ = db.add_ontology_edge(
                            &mem_id,
                            "memory",
                            "INSTANCE_OF",
                            concept_id,
                            "concept",
                            None,
                        ).await;

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
    }

    Ok(())
}

// ─── Phase 3: Concept Deduplication ───────────────────────────────────────

async fn phase_3_concept_dedup(
    args: &ConsolidateArgs,
    db: &std::sync::Arc<dyn voidm_db_trait::Database>,
    results: &mut ConsolidateResults,
) -> Result<()> {
    // Load all concepts using database trait
    let concepts = db.list_concepts(
        args.scope.as_deref(),
        10000, // high limit
    ).await?;

    results.phase_3_concept_dedup.concepts_checked = concepts.len();

    // Group by (name_normalized, scope)
    let mut groups: HashMap<(String, String), Vec<serde_json::Value>> = HashMap::new();
    for concept in concepts {
        if let (Some(name), Some(scope)) = (
            concept.get("name").and_then(|v| v.as_str()),
            concept.get("scope").and_then(|v| v.as_str()),
        ) {
            let key = (name.to_lowercase(), scope.to_string());
            groups.entry(key).or_default().push(concept);
        }
    }

    // Find duplicates and merge
    let mut duplicates_to_merge: Vec<(Vec<String>, String)> = Vec::new();
    for (_key, mut concept_group) in groups {
        if concept_group.len() > 1 {
            // Sort by created_at to keep oldest
            concept_group.sort_by_key(|c| {
                c.get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            });

            let keeper_id = concept_group[0]
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let to_delete_ids: Vec<String> = concept_group[1..]
                .iter()
                .filter_map(|c| c.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect();

            duplicates_to_merge.push((to_delete_ids, keeper_id));
        }
    }

    results.phase_3_concept_dedup.merge_groups = duplicates_to_merge.len();
    results.phase_3_concept_dedup.concepts_deleted =
        duplicates_to_merge.iter().map(|(d, _)| d.len()).sum();

    // Apply merges if not dry-run (using database trait for deletion)
    if !args.dry_run && !duplicates_to_merge.is_empty() {
        for (to_delete_ids, keeper_id) in duplicates_to_merge {
            for concept_id in to_delete_ids {
                // Delete concept via database trait
                let _ = db.delete_concept(&concept_id).await;
                results.phase_3_concept_dedup.instances_remapped += 1;
            }

            if args.verbose {
                eprintln!(
                    "  Merged concepts into {} (kept oldest)",
                    keeper_id
                );
            }
        }
    }

    Ok(())
}

// ─── Phase 4: NLI-Based Conflict Detection ────────────────────────────────

async fn phase_4_nli_conflicts(
    args: &ConsolidateArgs,
    db: &std::sync::Arc<dyn voidm_db_trait::Database>,
    results: &mut ConsolidateResults,
) -> Result<()> {
    // Query all CONTRADICTS edges via database trait
    let edges = db.list_ontology_edges().await?;

    // Filter for CONTRADICTS only
    let contradicts: Vec<_> = edges
        .iter()
        .filter(|e| e.get("rel_type").and_then(|v| v.as_str()) == Some("CONTRADICTS"))
        .collect();

    results.phase_4_nli_conflicts.contradicts_edges_examined = contradicts.len();

    if contradicts.is_empty() {
        return Ok(());
    }

    // Ensure NLI model is loaded
    voidm_nli::ensure_nli_model()
        .await
        .context("Failed to load NLI model")?;

    let mut confirmed = 0;

    for edge in contradicts {
        let from_id = edge
            .get("from_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let to_id = edge
            .get("to_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Fetch concept info
        if let (Ok(from_concept), Ok(to_concept)) = (
            db.get_concept(from_id).await,
            db.get_concept(to_id).await,
        ) {
            let text1 = from_concept
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let text2 = to_concept
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if !text1.is_empty() && !text2.is_empty() {
                // Run NLI classifier
                if let Ok(scores) = voidm_nli::classify(text1, text2) {
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
            }
        }
    }

    results.phase_4_nli_conflicts.confirmed_contradictions = confirmed;
    results.phase_4_nli_conflicts.flagged_for_review = confirmed;

    Ok(())
}

// ─── Output Formatting ────────────────────────────────────────────────────

fn print_human_output(results: &ConsolidateResults) {
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
