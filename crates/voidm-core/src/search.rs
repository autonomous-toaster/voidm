use anyhow::Result;
use sqlx::SqlitePool;
use crate::models::{Memory, SuggestedLink, edge_hint};
use voidm_db_trait::Database;

const NEIGHBOR_MAX_DEPTH: u8 = 3;
const NEVER_TRAVERSE: &[&str] = &["CONTRADICTS", "INVALIDATES"];

/// Search result with all signals merged.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub importance: i64,
    pub created_at: String,
    /// "search" for direct hits, "graph" for neighbor-expanded results.
    pub source: String,
    /// Only set for source="graph".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel_type: Option<String>,
    /// Only set for source="graph": "outgoing" | "incoming" | "undirected".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    /// Only set for source="graph".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hop_depth: Option<u8>,
    /// Only set for source="graph": ID of the direct search result this was reached from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Quality score (0.0-1.0) based on content genericity, abstraction, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<f32>,
    /// Optional title field for brief summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SearchMode {
    /// RRF (Reciprocal Rank Fusion) - the only ranking method.
    /// All search modes map to this for backward compatibility.
    Rrf,
}

impl std::str::FromStr for SearchMode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            // All modes map to RRF (first-class, only method)
            "hybrid" | "hybrid-rrf" | "rrf" | "semantic" | "keyword" | "fuzzy" | "bm25" => {
                Ok(SearchMode::Rrf)
            }
            other => Err(anyhow::anyhow!(
                "Unknown search mode: '{}'. All modes map to RRF (Reciprocal Rank Fusion): hybrid, rrf, semantic, keyword, fuzzy, bm25",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchOptions {
    pub query: String,
    pub mode: SearchMode,
    pub limit: usize,
    pub scope_filter: Option<String>,
    pub type_filter: Option<String>,
    /// Only applied in hybrid mode. None = use config default.
    pub min_score: Option<f32>,
    /// Minimum quality score (0.0-1.0) for results. None = no filter.
    pub min_quality: Option<f32>,
    /// If true, expand results with graph neighbors.
    pub include_neighbors: bool,
    /// Max hops for neighbor expansion (hard cap: NEIGHBOR_MAX_DEPTH).
    pub neighbor_depth: Option<u8>,
    /// Score decay per hop: neighbor_score = parent_score * decay^depth.
    pub neighbor_decay: Option<f32>,
    /// Min score for neighbors to be included.
    pub neighbor_min_score: Option<f32>,
    /// Max total neighbors to append (prevents hub explosion). None = same as limit.
    pub neighbor_limit: Option<usize>,
    /// Edge types to traverse. None = use config defaults.
    pub edge_types: Option<Vec<String>>,
    /// Optional intent/context for query expansion guidance.
    pub intent: Option<String>,
}

/// Result of a search, including threshold metadata for empty-result hints.
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    /// Set when threshold was applied and filtered some results out.
    pub threshold_applied: Option<f32>,
    /// Best score seen before threshold filtering (None if no results at all).
    pub best_score: Option<f32>,
}

/// Calculate title relevance score for a query match
/// 
/// Scoring:
/// - Exact title match: 2.0
/// - Title starts with query: 1.5
/// - Query substring in title: 1.0
/// - No match: 0.0
fn calculate_title_relevance(title: &Option<String>, query: &str) -> f32 {
    match title {
        None => 0.0,
        Some(t) => {
            let title_lower = t.to_lowercase();
            let query_lower = query.to_lowercase();
            
            // Exact match
            if title_lower == query_lower {
                2.0
            }
            // Prefix match
            else if title_lower.starts_with(&query_lower) {
                1.5
            }
            // Substring match
            else if title_lower.contains(&query_lower) {
                1.0
            }
            // No match
            else {
                0.0
            }
        }
    }
}

/// Rerank results by title relevance using weighted formula
/// 
/// Formula: (title_relevance * 0.3) + (existing_score * 0.7)
/// This gives 30% weight to title matches, 70% to content relevance
fn rerank_by_title_relevance(results: &mut Vec<SearchResult>, query: &str) {
    for result in results.iter_mut() {
        let title_score = calculate_title_relevance(&result.title, query);
        // Combine with existing score: 30% title, 70% content
        let new_score = (title_score * 0.3) + (result.score * 0.7);
        result.score = new_score;
    }
    
    // Re-sort by combined score
    results.sort_by(|a, b| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Full hybrid search pipeline.
pub async fn search(
    db: &dyn voidm_db_trait::Database,
    opts: &SearchOptions,
    model_name: &str,
    embeddings_enabled: bool,
    config_min_score: f32,
    config_search: &crate::config::SearchConfig,
) -> Result<SearchResponse> {
    use std::collections::HashMap;

    // RRF (Reciprocal Rank Fusion) is the ONLY ranking method.
    // All search is RRF. Configuration determines which signals to include.
    tracing::info!("Search: Starting RRF search request");
    tracing::debug!("Search: query='{}', limit={}", opts.query, opts.limit);

    // Adaptive fetch multiplier based on query complexity
    let query_complexity = crate::query_classifier::classify_query(&opts.query);
    let base_multiplier = 10u32; // Default 10x
    let adaptive_multiplier = query_complexity.fetch_multiplier(base_multiplier) as usize;
    
    tracing::debug!("Search: query_complexity={:?}, adaptive_multiplier={}x", 
        query_complexity, 
        adaptive_multiplier);

    // Pipeline: fetch (top X) → RRF (top Y) → reranking (top Z) → return (top K)
    // Increase fetch_limit if reranking is enabled to give it more candidates
    let fetch_limit = if config_search.reranker.as_ref().map_or(false, |r| r.enabled) {
        (opts.limit * 5).max(config_search.reranker.as_ref().map(|r| r.apply_to_top_k * 2).unwrap_or(30))
    } else {
        opts.limit * adaptive_multiplier  // Adaptive: 5x-20x based on query complexity
    };
    
    tracing::debug!("Search: fetch_limit={} (reranker enabled: {}, complexity: {:?})", 
        fetch_limit, 
        config_search.reranker.as_ref().map_or(false, |r| r.enabled),
        query_complexity);
    
    // Determine which signals to compute (from config)
    let use_vector = embeddings_enabled && config_search.signals.vector;
    let use_bm25 = config_search.signals.bm25;
    let use_fuzzy = config_search.signals.fuzzy;
    
    tracing::info!("Search: Signals enabled - vector: {}, bm25: {}, fuzzy: {}", use_vector, use_bm25, use_fuzzy);

    // ===== PARALLEL: Vector + BM25 (independent operations) =====
    let (vector_results, bm25_results) = tokio::join!(
        // Vector signal (embedding + ANN)
        async {
            let mut results = Vec::new();
            if use_vector {
                tracing::debug!("Search: Computing vector signal");
                if let Ok(embedding) = crate::embeddings::embed_text(model_name, &opts.query) {
                    if let Ok(rows) = db.search_ann(
                        embedding,
                        fetch_limit,
                        opts.scope_filter.as_deref(),
                        opts.type_filter.as_deref(),
                    ).await {
                        results = rows;
                        results.sort_by(|a, b| {
                            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        tracing::debug!("Search: Vector signal returned {} results", results.len());
                    } else {
                        tracing::warn!("Search: Vector ANN search failed");
                    }
                } else {
                    tracing::warn!("Search: Failed to embed query for vector signal");
                }
            }
            results
        },
        // BM25 signal (FTS5 query)
        async {
            let mut results = Vec::new();
            if use_bm25 {
                if let Ok(rows) = db.search_bm25(
                    &opts.query,
                    opts.scope_filter.as_deref(),
                    opts.type_filter.as_deref(),
                    fetch_limit,
                ).await {
                    results = rows;
                    tracing::debug!("Search: BM25 returned {} results", results.len());
                    results.sort_by(|a, b| {
                        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                    });
                } else {
                    tracing::warn!("Search: BM25 search failed");
                }
            }
            results
        }
    );

    // --- Fuzzy Signal (sequential) ---
    let mut fuzzy_results = Vec::new();
    if use_fuzzy {
        tracing::debug!("Search: Computing fuzzy signal");
        if let Ok(results) = db.search_fuzzy(
            &opts.query,
            opts.scope_filter.as_deref(),
            fetch_limit,
            0.6,
        ).await {
            fuzzy_results = results;
            fuzzy_results.sort_by(|a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            });
            tracing::debug!("Search: Fuzzy signal returned {} results", fuzzy_results.len());
        }
    }

    // Prepare signals for RRF fusion
    let mut signals: Vec<(&str, Vec<(String, f32)>)> = Vec::new();
    let vector_len = vector_results.len();
    let bm25_len = bm25_results.len();
    let fuzzy_len = fuzzy_results.len();
    
    tracing::info!("Search: Signal collection - vector: {}, bm25: {}, fuzzy: {}", vector_len, bm25_len, fuzzy_len);
    
    if !vector_results.is_empty() {
        signals.push(("vector", vector_results));
    }
    if !bm25_results.is_empty() {
        signals.push(("bm25", bm25_results));
    }
    if !fuzzy_results.is_empty() {
        signals.push(("fuzzy", fuzzy_results));
    }

    tracing::info!("Search: Total signals for RRF fusion: {}", signals.len());

    if signals.is_empty() {
        // Fallback: return newest memories
        tracing::warn!("Search: No signals collected, falling back to newest memories");
        let memories = fetch_memories_newest(db, opts).await?;
        return Ok(SearchResponse {
            results: memories,
            threshold_applied: None,
            best_score: None,
        });
    }

    // Apply RRF fusion
    let rrf = crate::rrf_fusion::RRFFusion::default();
    let fused = rrf.fuse(signals);

    tracing::debug!("Search: RRF fusion complete, {} results", fused.len());

    // Fetch full memory records (before reranking, fetch more results)
    let mut results = Vec::new();
    let mut best_score = None;
    
    // RRF filter: take top (limit * 2) from RRF, then reranking will filter to final K
    let rrf_limit = if config_search.reranker.is_some() {
        (opts.limit * 3).min(fused.len())  // Fetch 3x for reranking to choose from
    } else {
        (opts.limit * 2).min(fused.len())
    };

    for rrf_result in fused.iter().take(rrf_limit) {
        if let Some(m) = fetch_memory_by_id(db, &rrf_result.id).await? {
            if let Some(ref scope) = opts.scope_filter {
                if !m.scopes.iter().any(|s| s.starts_with(scope.as_str())) {
                    continue;
                }
            }
            if let Some(ref t) = opts.type_filter {
                if m.memory_type != *t {
                    continue;
                }
            }

            let importance_boost = (m.importance as f32 - 5.0) * 0.02;
            // RRF scores in [0.01, 0.2] are too small. Scale to [0.2, 0.9] for visibility.
            let mut final_score = 0.2 + (rrf_result.rrf_score * 3.5).min(0.7) + importance_boost;
            
            if let Some(ref meta_config) = config_search.metadata_ranking {
                final_score = apply_metadata_ranking(&m, final_score, meta_config);
            }
            
            best_score = Some(best_score.unwrap_or(final_score).max(final_score));

            results.push(SearchResult {
                id: rrf_result.id.clone(),
                score: final_score,
                memory_type: m.memory_type,
                content: m.content,
                scopes: m.scopes,
                tags: m.tags,
                importance: m.importance,
                created_at: m.created_at,
                source: "search".into(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: m.quality_score,
                title: m.title,
            });

            if results.len() >= rrf_limit {
                break;
            }
        }
    }
    
    // Apply context-aware score boosting if query has intent
    let context_boost_config = crate::context_boosting::ContextBoostConfig::default();
    crate::context_boosting::boost_by_context(&mut results, opts.intent.as_deref(), &context_boost_config);
    
    // Apply importance-based boosting for better precision
    let importance_boost_config = crate::importance_boosting::ImportanceBoostConfig::default();
    crate::importance_boosting::boost_by_importance(&mut results, &importance_boost_config);
    
    // Re-sort results after boosting
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    
    // Apply reranking if configured
    #[cfg(feature = "reranker")]
    if let Some(reranker_config) = &config_search.reranker {
        if reranker_config.enabled {
            tracing::info!("Search: Applying reranking to {} results before final filtering", results.len());
            if let Err(e) = apply_reranker(reranker_config, &opts.query, &mut results).await {
                tracing::warn!("Search: Reranking failed, continuing with RRF scores: {}", e);
            }
        }
    }
    
    // Apply quality-based filtering to improve result reliability
    let quality_filter_config = crate::quality_filtering::QualityFilterConfig::default();
    crate::quality_filtering::filter_by_quality(&mut results, &quality_filter_config);
    
    // Apply recency-based boosting to surface fresher content
    let recency_boost_config = crate::recency_boosting::RecencyBoostConfig::default();
    crate::recency_boosting::boost_by_recency(&mut results, &recency_boost_config);
    
    // Re-sort after recency boosting
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    
    // Final filter to return only top-K results
    results.truncate(opts.limit);

    // Apply graph-aware retrieval if enabled (post-RRF expansion)
    if let Some(graph_config) = &config_search.graph_retrieval {
        if graph_config.enabled {
            if !results.is_empty() {
                tracing::info!("Search: Applying graph-aware retrieval to {} results", results.len());
                if let Err(e) = crate::graph_retrieval::expand_graph_results(db as &dyn voidm_db_trait::Database, &mut results, graph_config).await {
                    tracing::warn!("Search: Graph-aware retrieval failed: {}", e);
                }
            }
        }
    }

    // Apply title-based reranking to boost title matches
    tracing::debug!("Search: Applying title-based reranking");
    rerank_by_title_relevance(&mut results, &opts.query);
    
    tracing::info!("Search: Returning {} results", results.len());

    Ok(SearchResponse {
        results,
        threshold_applied: None,
        best_score,
    })
}

/// Expand search results with graph neighbors in-place.
async fn expand_neighbors(
    db: &dyn voidm_db_trait::Database,
    _results: &mut Vec<SearchResult>,
    _opts: &SearchOptions,
    _config: &crate::config::SearchConfig,
) -> Result<()> {
    // Note: Neighbor expansion requires graph_neighbors which uses pool directly
    // TODO: Implement via Database trait when voidm_graph is refactored
    Ok(())
}
async fn fetch_memories_newest(db: &dyn voidm_db_trait::Database, opts: &SearchOptions) -> Result<Vec<SearchResult>> {    
    let memories_json = db.list_memories(Some(opts.limit)).await?;
    let mut results = Vec::new();
    for memory_json in memories_json {
        let memory: Memory = serde_json::from_value(memory_json)?;
        results.push(SearchResult {
            id: memory.id,
            score: 0.35, // Fallback score for unranked newest memories (above 0.3 threshold)
            memory_type: memory.memory_type,
            content: memory.content,
            scopes: memory.scopes,
            tags: memory.tags,
            importance: memory.importance,
            created_at: memory.created_at,
            source: "search".into(),
            rel_type: None,
            direction: None,
            hop_depth: None,
            parent_id: None,
            quality_score: None,
            title: memory.title,
        });
    }
    Ok(results)
}

async fn fetch_memory_by_id(db: &dyn voidm_db_trait::Database, id: &str) -> Result<Option<Memory>> {
    if let Some(memory_json) = db.get_memory(id).await? {
        let memory: Memory = serde_json::from_value(memory_json)?;
        Ok(Some(memory))
    } else {
        Ok(None)
    }
}

pub fn sanitize_fts_query(q: &str) -> String {
    // FTS5 requires quoting special chars; simple approach: wrap in quotes
    let cleaned: String = q.chars()
        .map(|c| if c == '"' { ' ' } else { c })
        .collect();
    format!("\"{}\"", cleaned)
}

/// Find similar memories for suggested_links and duplicate detection.
pub async fn find_similar(
    pool: &SqlitePool,
    embedding: &[f32],
    exclude_id: &str,
    limit: usize,
    threshold: f32,
) -> Result<Vec<(String, f32)>> {
    if !crate::vector::vec_table_exists(pool).await? {
        return Ok(vec![]);
    }
    let all = crate::vector::ann_search(pool, embedding, limit + 1).await?;
    let results = all.into_iter()
        .filter(|(id, _)| id != exclude_id)
        .map(|(id, dist)| {
            let sim = 1.0 - (dist / 2.0).clamp(0.0, 1.0);
            (id, sim)
        })
        .filter(|(_, sim)| *sim >= threshold)
        .take(limit)
        .collect();
    Ok(results)
}

/// Build SuggestedLink entries from similar memories.
pub async fn build_suggested_links(
    pool: &SqlitePool,
    new_memory_type: &str,
    similar: Vec<(String, f32)>,
) -> Result<Vec<SuggestedLink>> {
    let mut links = Vec::new();
    for (id, score) in similar {
        if let Some(m) = crate::crud::get_memory(pool, &id).await? {
            let content_truncated = if m.content.len() > 120 {
                format!("{}...", safe_truncate(&m.content, 120))
            } else {
                m.content.clone()
            };
            let hint = format!(
                "High similarity ({:.2}) — consider: {}",
                score,
                edge_hint(new_memory_type, &m.memory_type)
            );
            links.push(SuggestedLink {
                id,
                score,
                memory_type: m.memory_type,
                content: content_truncated,
                hint,
            });
        }
    }
    Ok(links)
}

/// Truncate a string at a safe Unicode char boundary.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

/// Apply reranker to top-k results using pure reranker-guided scoring.
/// Uses only reranker scores for reranked results (no blending with original scores).
/// This aligns with the reranker's intent as an expert ranking override.
#[cfg(feature = "reranker")]
async fn apply_reranker(
    config: &crate::config::RerankerConfig,
    query: &str,
    results: &mut Vec<SearchResult>,
) -> anyhow::Result<()> {
    let apply_to_k = config.apply_to_top_k.min(results.len());
    if apply_to_k == 0 {
        tracing::info!("Reranker: apply_to_top_k=0, skipping reranking");
        return Ok(());
    }

    tracing::info!("Reranker: Initializing reranking with model: {}", config.model);
    tracing::debug!("Reranker config: apply_to_top_k={}", config.apply_to_top_k);
    
    let reranker = crate::reranker::CrossEncoderReranker::load(&config.model).await?;
    tracing::info!("Reranker: Model '{}' loaded successfully", config.model);
    
    // Extract passages using intelligent passage extraction
    let docs_to_rerank: Vec<String> = results[..apply_to_k]
        .iter()
        .map(|r| voidm_embeddings::passage::extract_best_passage(
            &r.content,
            query,
            &config.passage_extraction,
        ))
        .collect();
    
    let docs_to_rerank_refs: Vec<&str> = docs_to_rerank.iter().map(|s| s.as_str()).collect();

    tracing::debug!("Reranker: Starting reranking of top-{} results (from {} total)", apply_to_k, results.len());
    
    let reranked = reranker.rerank(query, &docs_to_rerank_refs)?;
    tracing::info!("Reranker: Successfully reranked {} documents", reranked.len());

    // Create a mapping of original_index -> reranker_score
    let mut rerank_scores: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
    let mut score_changes = Vec::new();
    
    for rerank_result in reranked {
        rerank_scores.insert(rerank_result.index, rerank_result.score);
    }

    // Update scores with pure reranker scores (no blending)
    for (idx, result) in results[..apply_to_k].iter_mut().enumerate() {
        if let Some(rerank_score) = rerank_scores.get(&idx) {
            let original_score = result.score;
            let score_delta = rerank_score - original_score;
            result.score = *rerank_score;  // Use pure reranker score
            
            tracing::debug!(
                "Reranked [{}]: {:.4} → {:.4} (Δ {:.4}, {:.1}%) | {}", 
                idx, 
                original_score, 
                rerank_score,
                score_delta,
                if original_score > 0.0 { (score_delta / original_score * 100.0) } else { 0.0 },
                &result.id[..std::cmp::min(12, result.id.len())]
            );
            
            score_changes.push((original_score, *rerank_score));
        }
    }

    // Calculate statistics
    if !score_changes.is_empty() {
        let original_mean = score_changes.iter().map(|(o, _)| o).sum::<f32>() / score_changes.len() as f32;
        let reranked_mean = score_changes.iter().map(|(_, r)| r).sum::<f32>() / score_changes.len() as f32;
        let min_original = score_changes.iter().map(|(o, _)| o).copied().fold(f32::INFINITY, f32::min);
        let max_reranked = score_changes.iter().map(|(_, r)| r).copied().fold(f32::NEG_INFINITY, f32::max);
        
        tracing::info!(
            "Reranker: Score statistics - Original (mean={:.4}, min={:.4}) → Reranked (mean={:.4}, max={:.4})",
            original_mean, min_original, reranked_mean, max_reranked
        );
    }

    // Re-sort by reranker scores
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    tracing::info!("Reranker: Results re-sorted by reranker scores");

    Ok(())
}

/// Enhanced hybrid search with Reciprocal Rank Fusion (RRF).
///
/// Combines vector, BM25, and fuzzy signals using RRF instead of weighted averaging.
/// Benefits:
/// - Better ranking by combining signals without manual weights
/// - Preserves high-confidence matches (rank 1-3 bonuses)
/// - Prevents any single signal from dominating
///
/// Usage: Enable via SearchMode or config option
// ─── Metadata Ranking Signals ───────────────────────────────────────────────

/// Compute recency signal: exp(-days_since_created / half_life)
pub fn compute_recency(created_at: &str, half_life_days: u32) -> f32 {
    if let Ok(created) = chrono::DateTime::parse_from_rfc3339(created_at) {
        let now = chrono::Utc::now();
        let days_since = (now.timestamp() - created.timestamp()) as f64 / 86400.0;
        (-days_since / half_life_days as f64).exp() as f32
    } else {
        1.0
    }
}

/// Quality signal: use quality_score directly, default 1.0
pub fn compute_quality(quality_score: Option<f32>) -> f32 {
    quality_score.unwrap_or(1.0).max(0.0).min(1.0)
}

/// Author trust multiplier: user=1.0, assistant=0.6, unknown=0.3
pub fn compute_author_trust(author: Option<&str>) -> f32 {
    match author.unwrap_or("user") {
        "assistant" => 0.6,
        "unknown" => 0.3,
        _ => 1.0,
    }
}

/// Citation signal: logarithmic with diminishing returns
/// 1 citation = 0.11, 10 = 1.0, 100 = 1.0 (saturates)
pub fn compute_citation_boost(count: u32) -> f32 {
    ((count as f64 + 1.0).ln() / (11.0_f64).ln()) as f32
}

/// Source reliability multiplier
pub fn compute_source_reliability(source: Option<&str>, boosts: &std::collections::HashMap<String, f32>) -> f32 {
    boosts.get(source.unwrap_or("unknown"))
        .copied()
        .unwrap_or(0.0)
}

pub fn apply_metadata_ranking(
    memory: &Memory,
    base_score: f32,
    config: &crate::config::MetadataRankingConfig,
) -> f32 {
    let author = memory.metadata
        .get("author")
        .and_then(|v| v.as_str());
    
    let source = memory.metadata
        .get("source_reliability")
        .and_then(|v| v.as_str());

    let quality_signal = compute_quality(memory.quality_score);
    let recency_signal = compute_recency(&memory.created_at, config.recency_half_life_days);
    let author_boost = compute_author_trust(author);
    let source_boost = compute_source_reliability(source, &config.source_reliability_boost);

    base_score
        + config.weight_quality * quality_signal
        + config.weight_recency * recency_signal
        + config.weight_author * author_boost
        + config.weight_source * source_boost
}

/// Query citation count for a memory (references from/to graph edges)
pub async fn query_citation_count(
    db: &dyn voidm_db_trait::Database,
    memory_id: &str,
) -> u32 {
    // TODO: Phase 4 - Implement via database trait method
    // Query: SELECT COUNT(*) FROM graph_edges WHERE target_id = ?
    // Optimization: Use batch query for multiple memories
    // Index available: idx_graph_edges_target
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_title_relevance_none() {
        let result = calculate_title_relevance(&None, "test");
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_calculate_title_relevance_exact_match() {
        let result = calculate_title_relevance(&Some("Database Optimization".to_string()), "Database Optimization");
        assert_eq!(result, 2.0);
    }

    #[test]
    fn test_calculate_title_relevance_exact_match_case_insensitive() {
        let result = calculate_title_relevance(&Some("Database Optimization".to_string()), "database optimization");
        assert_eq!(result, 2.0);
    }

    #[test]
    fn test_calculate_title_relevance_prefix_match() {
        let result = calculate_title_relevance(&Some("Database Optimization Techniques".to_string()), "Database Optimization");
        assert_eq!(result, 1.5);
    }

    #[test]
    fn test_calculate_title_relevance_substring_match() {
        let result = calculate_title_relevance(&Some("Advanced Database Optimization".to_string()), "Database");
        assert_eq!(result, 1.0);
    }

    #[test]
    fn test_calculate_title_relevance_no_match() {
        let result = calculate_title_relevance(&Some("Rust Programming".to_string()), "Python");
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_rerank_by_title_relevance_exact_match_boosts() {
        let mut results = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.5,
                memory_type: "semantic".to_string(),
                content: "Some content".to_string(),
                scopes: vec![],
                tags: vec![],
                importance: 5,
                created_at: "2024-01-01".to_string(),
                source: "search".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
                title: Some("Database".to_string()),
            },
            SearchResult {
                id: "2".to_string(),
                score: 0.7,
                memory_type: "semantic".to_string(),
                content: "Other content".to_string(),
                scopes: vec![],
                tags: vec![],
                importance: 5,
                created_at: "2024-01-02".to_string(),
                source: "search".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
                title: None,
            },
        ];

        rerank_by_title_relevance(&mut results, "Database");
        
        // Result 1 has exact title match: (2.0 * 0.3) + (0.5 * 0.7) = 0.6 + 0.35 = 0.95
        // Result 2 has no title match: (0.0 * 0.3) + (0.7 * 0.7) = 0.0 + 0.49 = 0.49
        // Result 1 should rank first
        assert!(results[0].id == "1");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_rerank_by_title_relevance_sorting() {
        let mut results = vec![
            SearchResult {
                id: "a".to_string(),
                score: 0.8,
                memory_type: "semantic".to_string(),
                content: "Content A".to_string(),
                scopes: vec![],
                tags: vec![],
                importance: 5,
                created_at: "2024-01-01".to_string(),
                source: "search".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
                title: None,
            },
            SearchResult {
                id: "b".to_string(),
                score: 0.6,
                memory_type: "semantic".to_string(),
                content: "Content B".to_string(),
                scopes: vec![],
                tags: vec![],
                importance: 5,
                created_at: "2024-01-02".to_string(),
                source: "search".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
                title: Some("Test".to_string()),
            },
        ];

        rerank_by_title_relevance(&mut results, "Test");
        
        // Result b has exact match: (2.0 * 0.3) + (0.6 * 0.7) = 0.6 + 0.42 = 1.02
        // Result a has no match: (0.0 * 0.3) + (0.8 * 0.7) = 0.0 + 0.56 = 0.56
        // Result b should rank first despite lower original score
        assert_eq!(results[0].id, "b");
        assert_eq!(results[1].id, "a");
    }
}

