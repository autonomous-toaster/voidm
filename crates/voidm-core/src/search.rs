use anyhow::Result;
use crate::models::Memory;
use crate::memory_policy::{
    RETRIEVAL_MAX_CHARS_PER_CHUNK,
    RETRIEVAL_MAX_CHUNKS_PER_MEMORY,
    RETRIEVAL_TOTAL_CHAR_BUDGET_PER_MEMORY,
};


/// Search result with all signals merged.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub id: String,
    /// Result object kind. Search returns memory-level results, not raw chunks.
    #[serde(rename = "object")]
    pub object_type: String,
    pub score: f32,
    #[serde(rename = "type")]
    pub memory_type: String,
    /// Assistant-facing bounded context preview assembled from top chunks or truncated memory content.
    pub content: String,
    /// True when `content` is a bounded preview rather than the full memory body.
    pub content_truncated: bool,
    /// Explains where `content` came from: `context_chunks` or `memory_truncate`.
    pub content_source: String,
    /// Bounded chunk snippets used for context assembly
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub context_chunks: Vec<String>,
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
            "hybrid" | "hybrid-rrf" | "rrf" | "semantic" | "keyword" | "fuzzy" | "bm25" | "vector" => {
                Ok(SearchMode::Rrf)
            }
            other => Err(anyhow::anyhow!(
                "Unknown search mode: '{}'. All modes map to RRF (Reciprocal Rank Fusion): hybrid, rrf, semantic, keyword, fuzzy, bm25, vector",
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
    pub tag_filter: Option<Vec<String>>,
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

fn calculate_memory_type_relevance(memory_type: &str, query: &str) -> f32 {
    let mt = memory_type.to_lowercase();
    let q = query.to_lowercase();

    if q == mt {
        0.12
    } else if q.split_whitespace().any(|token| token == mt) {
        0.08
    } else if q.contains(&mt) {
        0.05
    } else {
        0.0
    }
}

fn derive_type_intent(query: &str, explicit_intent: Option<&str>) -> Option<String> {
    let mut haystacks = vec![query.to_lowercase()];
    if let Some(intent) = explicit_intent {
        haystacks.push(intent.to_lowercase());
    }

    for candidate in ["episodic", "semantic", "procedural", "conceptual", "contextual"] {
        if haystacks.iter().any(|h| h.split_whitespace().any(|token| token == candidate) || h.contains(candidate)) {
            return Some(candidate.to_string());
        }
    }

    None
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
    db: &dyn voidm_db::Database,
    opts: &SearchOptions,
    model_name: &str,
    embeddings_enabled: bool,
    _config_min_score: f32,
    config_search: &crate::config::SearchConfig,
) -> Result<SearchResponse> {

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
    let derived_type_intent = derive_type_intent(&opts.query, opts.intent.as_deref());
    
    tracing::info!("Search: Signals enabled - vector: {}, bm25: {}, fuzzy: {}", use_vector, use_bm25, use_fuzzy);

    // ===== PARALLEL: Chunk vector + content BM25 + title BM25 =====
    let (vector_results, bm25_results, title_results) = tokio::join!(
        // Vector signal (embedding + ANN)
        async {
            let mut results = Vec::new();
            if use_vector {
                tracing::debug!("Search: Computing vector signal");
                if let Ok(embedding) = crate::embeddings::embed_text(model_name, &opts.query) {
                    if let Ok(rows) = db.search_chunk_ann(
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
        },
        // Title lexical signal
        async {
            let mut results = Vec::new();
            if use_bm25 {
                if let Ok(rows) = db.search_title_bm25(
                    &opts.query,
                    opts.scope_filter.as_deref(),
                    opts.type_filter.as_deref(),
                    fetch_limit,
                ).await {
                    results = rows;
                    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
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
    let title_len = title_results.len();
    let fuzzy_len = fuzzy_results.len();
    
    tracing::info!("Search: Signal collection - vector(chunks): {}, bm25(content): {}, bm25(title): {}, fuzzy: {}", vector_len, bm25_len, title_len, fuzzy_len);
    
    if !vector_results.is_empty() {
        signals.push(("vector", vector_results));
    }
    if !bm25_results.is_empty() {
        signals.push(("bm25", bm25_results));
    }
    if !title_results.is_empty() {
        signals.push(("title", title_results));
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

    tracing::debug!("Search: RRF fusion complete, {} grouped results", fused.len());

    // Group by memory (chunk hits are chunk_ids, lexical/title hits are memory ids)
    let mut grouped: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut supporting_chunk_scores: std::collections::HashMap<String, Vec<(String, f32)>> = std::collections::HashMap::new();
    for item in fused {
        let is_chunk = item.id.starts_with("mchk_");
        let memory_id = if is_chunk {
            if let Some(chunk) = db.get_chunk(&item.id).await? {
                chunk.get("memory_id").and_then(|v| v.as_str()).unwrap_or(&item.id).to_string()
            } else {
                item.id.clone()
            }
        } else {
            item.id.clone()
        };
        grouped.entry(memory_id.clone())
            .and_modify(|s| *s = s.max(item.rrf_score))
            .or_insert(item.rrf_score);

        if is_chunk {
            supporting_chunk_scores
                .entry(memory_id)
                .or_default()
                .push((item.id, item.rrf_score));
        }
    }

    let mut grouped_vec: Vec<(String, f32)> = grouped.into_iter().collect();
    grouped_vec.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Fetch full memory records (before reranking, fetch more results)
    let mut results = Vec::new();
    let mut best_score = None;
    
    // RRF filter: take top (limit * 2) from RRF, then reranking will filter to final K
    let rrf_limit = if config_search.reranker.is_some() {
        (opts.limit * 3).min(grouped_vec.len())
    } else {
        (opts.limit * 2).min(grouped_vec.len())
    };

    for (memory_id, grouped_score) in grouped_vec.iter().take(rrf_limit) {
        if let Some(m) = fetch_memory_by_id(db, memory_id).await? {
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
            if let Some(ref tags) = opts.tag_filter {
                let all_present = tags.iter().all(|tag| m.tags.iter().any(|t| t == tag));
                if !all_present {
                    continue;
                }
            }

            let importance_boost = (m.importance as f32 - 5.0) * 0.01;
            let type_relevance_boost = calculate_memory_type_relevance(&m.memory_type, &opts.query)
                + if derived_type_intent.as_deref() == Some(m.memory_type.as_str()) { 0.05 } else { 0.0 };
            let tag_match_boost = opts.tag_filter.as_ref().map(|tags| {
                if tags.iter().all(|tag| m.tags.iter().any(|t| t == tag)) { 0.03 } else { 0.0 }
            }).unwrap_or(0.0);
            // Keep RRF as the main retrieval signal and use only light post-fusion boosts.
            let mut final_score = 0.15 + (*grouped_score * 2.5).min(0.55) + importance_boost + type_relevance_boost + tag_match_boost;
            
            if let Some(ref meta_config) = config_search.metadata_ranking {
                final_score = apply_metadata_ranking(&m, final_score, meta_config);
            }
            
            best_score = Some(best_score.unwrap_or(final_score).max(final_score));

            let context_chunks = collect_context_chunks(
                db,
                memory_id,
                supporting_chunk_scores.get(memory_id),
                RETRIEVAL_MAX_CHUNKS_PER_MEMORY,
                RETRIEVAL_MAX_CHARS_PER_CHUNK,
                RETRIEVAL_TOTAL_CHAR_BUDGET_PER_MEMORY,
            ).await?;
            let (display_content, content_truncated, content_source) = if context_chunks.is_empty() {
                (
                    safe_truncate(&m.content, RETRIEVAL_MAX_CHARS_PER_CHUNK).to_string(),
                    m.content.chars().count() > RETRIEVAL_MAX_CHARS_PER_CHUNK,
                    "memory_truncate".to_string(),
                )
            } else {
                (
                    context_chunks.join("\n\n"),
                    true,
                    "context_chunks".to_string(),
                )
            };

            results.push(SearchResult {
                id: memory_id.clone(),
                object_type: "memory".to_string(),
                score: final_score,
                memory_type: m.memory_type,
                content: display_content,
                content_truncated,
                content_source,
                context_chunks,
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
    
    // Apply a light context-aware score boost if query has intent.
    // Avoid stacking multiple heavy post-RRF heuristic boosters before reranking.
    let context_boost_config = crate::context_boosting::ContextBoostConfig::default();
    crate::context_boosting::boost_by_context(&mut results, opts.intent.as_deref(), &context_boost_config);
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
    
    // Apply quality-based filtering to improve result reliability.
    let quality_filter_config = crate::quality_filtering::QualityFilterConfig::default();
    crate::quality_filtering::filter_by_quality(&mut results, &quality_filter_config);
    
    // Apply only a light recency tiebreak after reranking/filtering.
    let recency_boost_config = crate::recency_boosting::RecencyBoostConfig::default();
    crate::recency_boosting::boost_by_recency(&mut results, &recency_boost_config);
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Apply title-based reranking before final truncation so lexical ties settle predictably.
    tracing::debug!("Search: Applying title-based reranking");
    rerank_by_title_relevance(&mut results, &opts.query);
    
    // Final filter to return only top-K results
    results.truncate(opts.limit);

    // Apply graph-aware retrieval if enabled (post-RRF expansion)
    if let Some(graph_config) = &config_search.graph_retrieval {
        if graph_config.enabled {
            if !results.is_empty() {
                tracing::info!("Search: Applying graph-aware retrieval to {} results", results.len());
                if let Err(e) = crate::graph_retrieval::expand_graph_results(db as &dyn voidm_db::Database, &mut results, graph_config).await {
                    tracing::warn!("Search: Graph-aware retrieval failed: {}", e);
                }
            }
        }
    }

    tracing::info!("Search: Returning {} results", results.len());

    Ok(SearchResponse {
        results,
        threshold_applied: None,
        best_score,
    })
}

/// Expand search results with graph neighbors in-place.
#[allow(dead_code)]
async fn expand_neighbors(
    _db: &dyn voidm_db::Database,
    _results: &mut Vec<SearchResult>,
    _opts: &SearchOptions,
    _config: &crate::config::SearchConfig,
) -> Result<()> {
    // Note: Neighbor expansion requires graph_neighbors which uses pool directly
    // TODO: Implement via Database trait when voidm_graph is refactored
    Ok(())
}
async fn fetch_memories_newest(db: &dyn voidm_db::Database, opts: &SearchOptions) -> Result<Vec<SearchResult>> {    
    let memories_json = db.list_memories(Some(opts.limit)).await?;
    let mut results = Vec::new();
    for memory_json in memories_json {
        let memory: Memory = serde_json::from_value(memory_json)?;
        let fallback_content = safe_truncate(&memory.content, RETRIEVAL_MAX_CHARS_PER_CHUNK).to_string();
        let content_truncated = memory.content.chars().count() > RETRIEVAL_MAX_CHARS_PER_CHUNK;
        results.push(SearchResult {
            id: memory.id,
            object_type: "memory".to_string(),
            score: 0.35, // Fallback score for unranked newest memories (above 0.3 threshold)
            memory_type: memory.memory_type,
            content: fallback_content,
            content_truncated,
            content_source: "memory_truncate".to_string(),
            context_chunks: Vec::new(),
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

async fn fetch_memory_by_id(db: &dyn voidm_db::Database, id: &str) -> Result<Option<Memory>> {
    if let Some(memory_json) = db.get_memory(id).await? {
        let memory: Memory = serde_json::from_value(memory_json)?;
        Ok(Some(memory))
    } else {
        Ok(None)
    }
}

async fn collect_context_chunks(
    db: &dyn voidm_db::Database,
    memory_id: &str,
    supporting_chunks: Option<&Vec<(String, f32)>>,
    max_chunks: usize,
    max_chars_per_chunk: usize,
    total_budget: usize,
) -> Result<Vec<String>> {
    let mut selected = Vec::new();
    let mut total = 0usize;

    if let Some(chunks) = supporting_chunks {
        let mut ranked_chunks = chunks.clone();
        ranked_chunks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (chunk_id, _) in ranked_chunks {
            if selected.len() >= max_chunks { break; }
            if let Some(chunk) = db.get_chunk(&chunk_id).await? {
                let chunk_memory_id = chunk.get("memory_id").and_then(|v| v.as_str()).unwrap_or("");
                if chunk_memory_id != memory_id {
                    continue;
                }
                if let Some(text) = chunk.get("content").and_then(|v| v.as_str()) {
                    let trimmed: String = text.chars().take(max_chars_per_chunk).collect();
                    if total + trimmed.len() > total_budget { break; }
                    total += trimmed.len();
                    selected.push(trimmed);
                }
            }
        }
    }

    if selected.is_empty() {
        let all_chunks = db.fetch_chunks(10_000).await?;
        for (_chunk_id, text, _chunk_memory_id) in all_chunks.into_iter().filter(|(_, _, mid)| mid == memory_id) {
            if selected.len() >= max_chunks { break; }
            let trimmed: String = text.chars().take(max_chars_per_chunk).collect();
            if total + trimmed.len() > total_budget { break; }
            total += trimmed.len();
            selected.push(trimmed);
        }
    }

    Ok(selected)
}

pub fn sanitize_fts_query(q: &str) -> String {
    // FTS5 requires quoting special chars; simple approach: wrap in quotes
    let cleaned: String = q.chars()
        .map(|c| if c == '"' { ' ' } else { c })
        .collect();
    format!("\"{}\"", cleaned)
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
                if original_score > 0.0 { score_delta / original_score * 100.0 } else { 0.0 },
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
    _db: &dyn voidm_db::Database,
    _memory_id: &str,
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
                object_type: "memory".to_string(),
                score: 0.5,
                memory_type: "semantic".to_string(),
                content: "Some content".to_string(),
                content_truncated: false,
                content_source: "memory_truncate".to_string(),
                context_chunks: Vec::new(),
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
                object_type: "memory".to_string(),
                score: 0.7,
                memory_type: "semantic".to_string(),
                content: "Other content".to_string(),
                content_truncated: false,
                content_source: "memory_truncate".to_string(),
                context_chunks: Vec::new(),
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
                object_type: "memory".to_string(),
                score: 0.8,
                memory_type: "semantic".to_string(),
                content: "Content A".to_string(),
                content_truncated: false,
                content_source: "memory_truncate".to_string(),
                context_chunks: Vec::new(),
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
                object_type: "memory".to_string(),
                score: 0.6,
                memory_type: "semantic".to_string(),
                content: "Content B".to_string(),
                content_truncated: false,
                content_source: "memory_truncate".to_string(),
                context_chunks: Vec::new(),
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
        
        assert_eq!(results[0].id, "b");
        assert_eq!(results[1].id, "a");
    }

    #[test]
    fn test_retrieval_budget_constants_are_consistent() {
        assert!(RETRIEVAL_MAX_CHUNKS_PER_MEMORY <= crate::memory_policy::RETRIEVAL_MAX_CHUNKS);
        assert!(RETRIEVAL_TOTAL_CHAR_BUDGET_PER_MEMORY <= crate::memory_policy::RETRIEVAL_TOTAL_CHAR_BUDGET);
        assert!(RETRIEVAL_MAX_CHARS_PER_CHUNK > 0);
    }
}

