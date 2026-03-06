use anyhow::Result;
use sqlx::SqlitePool;
use crate::models::{Memory, SuggestedLink, edge_hint};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchMode {
    Hybrid,
    Semantic,
    Keyword,
    Fuzzy,
    Bm25,
}

impl std::str::FromStr for SearchMode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hybrid" => Ok(SearchMode::Hybrid),
            "semantic" => Ok(SearchMode::Semantic),
            "keyword" => Ok(SearchMode::Keyword),
            "fuzzy" => Ok(SearchMode::Fuzzy),
            "bm25" => Ok(SearchMode::Bm25),
            other => Err(anyhow::anyhow!("Unknown search mode: '{}'. Valid: hybrid, semantic, keyword, fuzzy, bm25", other)),
        }
    }
}

pub struct SearchOptions {
    pub query: String,
    pub mode: SearchMode,
    pub limit: usize,
    pub scope_filter: Option<String>,
    pub type_filter: Option<String>,
    /// Only applied in hybrid mode. None = use config default.
    pub min_score: Option<f32>,
}

/// Result of a search, including threshold metadata for empty-result hints.
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    /// Set when threshold was applied and filtered some results out.
    pub threshold_applied: Option<f32>,
    /// Best score seen before threshold filtering (None if no results at all).
    pub best_score: Option<f32>,
}

/// Full hybrid search pipeline.
pub async fn search(
    pool: &SqlitePool,
    opts: &SearchOptions,
    model_name: &str,
    embeddings_enabled: bool,
    config_min_score: f32,
) -> Result<SearchResponse> {
    use std::collections::HashMap;

    let fetch_limit = opts.limit * 3; // over-fetch for merging
    let mut scores: HashMap<String, f32> = HashMap::new();

    // --- Vector ANN ---
    let use_vector = embeddings_enabled
        && matches!(opts.mode, SearchMode::Hybrid | SearchMode::Semantic)
        && crate::vector::vec_table_exists(pool).await.unwrap_or(false);

    if use_vector {
        match crate::embeddings::embed_text(model_name, &opts.query) {
            Ok(embedding) => {
                match crate::vector::ann_search(pool, &embedding, fetch_limit).await {
                    Ok(hits) => {
                        for (id, dist) in hits {
                            // Convert cosine distance [0,2] to similarity [0,1]
                            let sim = 1.0 - (dist / 2.0).clamp(0.0, 1.0);
                            *scores.entry(id).or_default() += sim * 0.5;
                        }
                    }
                    Err(e) => tracing::warn!("Vector search failed: {}", e),
                }
            }
            Err(e) => tracing::warn!("Embedding failed: {}", e),
        }
    }

    // --- BM25 via FTS5 ---
    let use_bm25 = matches!(opts.mode, SearchMode::Hybrid | SearchMode::Bm25 | SearchMode::Keyword);
    if use_bm25 {
        let fts_query = sanitize_fts_query(&opts.query);
        let rows: Vec<(String, f32)> = sqlx::query_as(
            "SELECT id, bm25(memories_fts) AS score FROM memories_fts WHERE content MATCH ? ORDER BY score LIMIT ?"
        )
        .bind(&fts_query)
        .bind(fetch_limit as i64)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        // BM25 scores are negative in FTS5 (more negative = more relevant)
        let min_bm25 = rows.iter().map(|(_, s)| *s).fold(f32::MAX, f32::min);
        let max_bm25 = rows.iter().map(|(_, s)| *s).fold(f32::MIN, f32::max);
        let range = (max_bm25 - min_bm25).abs().max(0.001);

        for (id, raw_score) in rows {
            // Normalize to [0, 1] where higher = more relevant (invert because BM25 is negative)
            let norm = 1.0 - ((raw_score - min_bm25) / range).clamp(0.0, 1.0);
            *scores.entry(id).or_default() += norm * 0.3;
        }
    }

    // --- Fuzzy (Jaro-Winkler) ---
    let use_fuzzy = matches!(opts.mode, SearchMode::Hybrid | SearchMode::Fuzzy);
    if use_fuzzy {
        let all: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, content FROM memories ORDER BY created_at DESC LIMIT 500"
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let query_lower = opts.query.to_lowercase();
        for (id, content) in all {
            let sim = strsim::jaro_winkler(&query_lower, &content.to_lowercase()) as f32;
            if sim > 0.6 {
                *scores.entry(id).or_default() += sim * 0.2;
            }
        }
    }

    if scores.is_empty() {
        // Fallback: return newest memories (no threshold applied — no scores to compare)
        let memories = fetch_memories_newest(pool, opts).await?;
        return Ok(SearchResponse {
            results: memories,
            threshold_applied: None,
            best_score: None,
        });
    }

    // Collect IDs sorted by score
    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(opts.limit);

    // Fetch full memory records for top results
    let mut results = Vec::new();
    for (id, score) in ranked {
        if let Some(m) = fetch_memory_by_id(pool, &id).await? {
            // Apply scope/type filters
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
            // Boost by importance
            let importance_boost = (m.importance as f32 - 5.0) * 0.02;
            results.push(SearchResult {
                id,
                score: score + importance_boost,
                memory_type: m.memory_type,
                content: m.content,
                scopes: m.scopes,
                tags: m.tags,
                importance: m.importance,
                created_at: m.created_at,
            });
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Apply threshold — only in hybrid mode
    if opts.mode == SearchMode::Hybrid {
        let threshold = opts.min_score.unwrap_or(config_min_score);
        let best_score = results.first().map(|r| r.score);
        let before_count = results.len();
        results.retain(|r| r.score >= threshold);

        let threshold_applied = if results.len() < before_count {
            Some(threshold)
        } else {
            None
        };

        return Ok(SearchResponse { results, threshold_applied, best_score });
    }

    Ok(SearchResponse { results, threshold_applied: None, best_score: None })
}

async fn fetch_memories_newest(pool: &SqlitePool, opts: &SearchOptions) -> Result<Vec<SearchResult>> {    let memories = crate::crud::list_memories(pool, opts.scope_filter.as_deref(), opts.type_filter.as_deref(), opts.limit).await?;
    Ok(memories.into_iter().map(|m| SearchResult {
        id: m.id,
        score: 0.0,
        memory_type: m.memory_type,
        content: m.content,
        scopes: m.scopes,
        tags: m.tags,
        importance: m.importance,
        created_at: m.created_at,
    }).collect())
}

async fn fetch_memory_by_id(pool: &SqlitePool, id: &str) -> Result<Option<Memory>> {
    crate::crud::get_memory(pool, id).await
}

fn sanitize_fts_query(q: &str) -> String {
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
