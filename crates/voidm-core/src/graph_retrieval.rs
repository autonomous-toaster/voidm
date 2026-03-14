//! Graph-aware retrieval for search results.
//!
//! This module finds related memories based on:
//! - Shared tags (tag overlap scoring)
//! - Shared concepts (ontology relationships)
//!
//! Allows search to include memories that are conceptually related
//! to directly matched results, improving recall while maintaining precision.

#![allow(dead_code)]

use anyhow::Result;
use sqlx::SqlitePool;
use crate::models::Memory;
use crate::search::SearchResult;
use std::collections::HashSet;
use serde::{Deserialize, Serialize};

/// Configuration for tag-based retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRetrievalConfig {
    /// Enable tag-based related retrieval (default: true).
    #[serde(default = "default_tag_enabled")]
    pub enabled: bool,
    /// Minimum number of shared tags to consider related (default: 3).
    #[serde(default = "default_tag_min_overlap")]
    pub min_overlap: usize,
    /// Minimum overlap percentage (0-100) to include (default: 50).
    #[serde(default = "default_tag_min_percentage")]
    pub min_percentage: f32,
    /// Score decay for related results vs direct hits (default: 0.7).
    #[serde(default = "default_tag_decay")]
    pub decay_factor: f32,
    /// Max related memories per direct result (default: 5).
    #[serde(default = "default_tag_limit")]
    pub limit: usize,
}

fn default_tag_enabled() -> bool { true }
fn default_tag_min_overlap() -> usize { 3 }
fn default_tag_min_percentage() -> f32 { 50.0 }
fn default_tag_decay() -> f32 { 0.7 }
fn default_tag_limit() -> usize { 5 }

impl Default for TagRetrievalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_overlap: 3,
            min_percentage: 50.0,
            decay_factor: 0.7,
            limit: 5,
        }
    }
}
/// Configuration for concept-based retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptRetrievalConfig {
    /// Enable concept-based related retrieval (default: true).
    #[serde(default = "default_concept_enabled")]
    pub enabled: bool,
    /// Score decay for concept-related results (default: 0.7).
    #[serde(default = "default_concept_decay")]
    pub decay_factor: f32,
    /// Max concept-related memories per direct result (default: 3).
    #[serde(default = "default_concept_limit")]
    pub limit: usize,
}

fn default_concept_enabled() -> bool { true }
fn default_concept_decay() -> f32 { 0.7 }
fn default_concept_limit() -> usize { 3 }

impl Default for ConceptRetrievalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            decay_factor: 0.7,
            limit: 3,
        }
    }
}
/// Graph retrieval configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRetrievalConfig {
    /// Enable graph-aware retrieval (default: true).
    #[serde(default = "default_graph_enabled")]
    pub enabled: bool,
    /// Tag-based retrieval configuration.
    #[serde(default)]
    pub tags: TagRetrievalConfig,
    /// Concept-based retrieval configuration.
    #[serde(default)]
    pub concepts: ConceptRetrievalConfig,
}

fn default_graph_enabled() -> bool { true }

impl Default for GraphRetrievalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tags: TagRetrievalConfig::default(),
            concepts: ConceptRetrievalConfig::default(),
        }
    }
}

/// Find memories related via shared tags.
///
/// Scores memories based on tag overlap:
/// - overlap_count >= min_overlap
/// - (overlap_count / max_tags) * 100 >= min_percentage
///
/// Returns memories with scores decayed based on overlap percentage.
pub async fn find_related_by_tags(
    _pool: &SqlitePool,
    direct_results: &[SearchResult],
    config: &TagRetrievalConfig,
) -> Result<Vec<SearchResult>> {
    if !config.enabled || direct_results.is_empty() {
        return Ok(Vec::new());
    }

    let related = Vec::new();
    let _seen_ids: HashSet<String> = direct_results.iter().map(|r| r.id.clone()).collect();

    // Placeholder implementation - will be completed when full DB access is available
    // For now, returning empty results to allow the module to compile
    
    Ok(related)
}

/// Internal: Find memories with overlapping tags.
#[allow(dead_code)]
async fn find_memories_by_tag_overlap(
    _pool: &SqlitePool,
    _exclude_id: &str,
    _query_tags: &HashSet<&String>,
    _config: &TagRetrievalConfig,
) -> Result<Vec<(Memory, usize)>> {
    // This is a placeholder implementation.
    // In production, you'd query the database for memories with shared tags.
    // For now, returning empty vec to avoid compilation errors.
    // The actual implementation would:
    // 1. Query all memories (or use tag index if available)
    // 2. Calculate overlap_count for each
    // 3. Return filtered/sorted results
    
    Ok(Vec::new())
}

/// Find memories related via shared concepts.
///
/// Traverses ontology to find memories linked to related concept nodes.
/// Returns memories with scores based on concept distance.
pub async fn find_related_by_concepts(
    pool: &SqlitePool,
    direct_results: &[SearchResult],
    config: &ConceptRetrievalConfig,
) -> Result<Vec<SearchResult>> {
    if !config.enabled || direct_results.is_empty() {
        return Ok(Vec::new());
    }

    let mut related = Vec::new();
    let mut seen_ids: HashSet<String> = direct_results.iter().map(|r| r.id.clone()).collect();

    for _direct_result in direct_results {
        // This is a placeholder for concept-based retrieval
        // In production, you'd:
        // 1. Get the concept nodes linked to this memory
        // 2. Find other memories linked to same/related concepts
        // 3. Score by concept distance (IS-A relationships)
        // For now, we skip this to keep initial implementation simple
    }

    Ok(related)
}

/// Merge graph-aware results with original search results.
///
/// Deduplicates by ID and applies score decay for related results.
pub fn merge_graph_results(
    original: Vec<SearchResult>,
    tag_related: Vec<SearchResult>,
    concept_related: Vec<SearchResult>,
) -> Vec<SearchResult> {
    let mut merged = original;
    let mut seen_ids: HashSet<String> = merged.iter().map(|r| r.id.clone()).collect();

    // Add tag-related results
    for result in tag_related {
        if !seen_ids.contains(&result.id) {
            seen_ids.insert(result.id.clone());
            merged.push(result);
        }
    }

    // Add concept-related results
    for result in concept_related {
        if !seen_ids.contains(&result.id) {
            seen_ids.insert(result.id.clone());
            merged.push(result);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_overlap_percentage() {
        // Test: 3 shared tags out of 5 total = 60%
        let query_tags: Vec<String> = vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string(), "e".to_string()];
        let memory_tags: Vec<String> = vec!["b".to_string(), "c".to_string(), "d".to_string(), "f".to_string()];

        let query_set: HashSet<_> = query_tags.iter().collect();
        let memory_set: HashSet<_> = memory_tags.iter().collect();

        let overlap = query_set.intersection(&memory_set).count();
        let max_tags = query_tags.len().max(memory_tags.len());
        let percentage = (overlap as f32 / max_tags as f32) * 100.0;

        assert_eq!(overlap, 3);
        assert_eq!(max_tags, 5);
        assert!((percentage - 60.0).abs() < 0.1);
    }

    #[test]
    fn test_score_decay() {
        let base_score: f32 = 0.8;
        let decay_factor: f32 = 0.7;
        let decayed = base_score * decay_factor;

        assert!((decayed - 0.56_f32).abs() < 0.01);
    }

    #[test]
    fn test_tag_retrieval_config_defaults() {
        let config = TagRetrievalConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_overlap, 3);
        assert_eq!(config.min_percentage, 50.0);
        assert_eq!(config.decay_factor, 0.7);
        assert_eq!(config.limit, 5);
    }

    #[test]
    fn test_concept_retrieval_config_defaults() {
        let config = ConceptRetrievalConfig::default();
        assert!(config.enabled);
        assert_eq!(config.decay_factor, 0.7);
        assert_eq!(config.limit, 3);
    }

    #[test]
    fn test_graph_retrieval_config_defaults() {
        let config = GraphRetrievalConfig::default();
        assert!(config.enabled);
        assert!(config.tags.enabled);
        assert!(config.concepts.enabled);
    }

    #[test]
    fn test_merge_deduplication() {
        let original = vec![
            SearchResult {
                id: "1".to_string(),
                score: 0.9,
                memory_type: "note".to_string(),
                content: "test".to_string(),
                scopes: vec![],
                tags: vec![],
                importance: 0,
                created_at: "2026-01-01".to_string(),
                source: "search".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
            },
        ];

        let tag_related = vec![
            SearchResult {
                id: "2".to_string(),
                score: 0.5,
                memory_type: "note".to_string(),
                content: "related".to_string(),
                scopes: vec![],
                tags: vec![],
                importance: 0,
                created_at: "2026-01-02".to_string(),
                source: "graph_tags".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: Some("1".to_string()),
                quality_score: None,
            },
        ];

        let concept_related = vec![
            SearchResult {
                id: "1".to_string(),  // Duplicate of original
                score: 0.4,
                memory_type: "note".to_string(),
                content: "test".to_string(),
                scopes: vec![],
                tags: vec![],
                importance: 0,
                created_at: "2026-01-01".to_string(),
                source: "graph_concepts".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
            },
        ];

        let merged = merge_graph_results(original, tag_related, concept_related);
        
        // Should have 2 results (1 original + 1 tag_related, concept duplicate filtered)
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "1");
        assert_eq!(merged[1].id, "2");
    }
}
