//! Importance-based score boosting for better precision and relevance ranking.
//!
//! High-importance memories are prioritized in results, improving precision by
//! ensuring more curated/valuable content ranks higher.

use crate::search::SearchResult;

/// Importance boost configuration.
#[derive(Debug, Clone)]
pub struct ImportanceBoostConfig {
    /// Enable importance boosting (default: true)
    pub enabled: bool,
    /// Score multiplier for high-importance results (default: 1.4)
    pub high_importance_boost: f32,
    /// Importance threshold (0-10) above which results get boosted (default: 7)
    pub importance_threshold: i64,
}

impl Default for ImportanceBoostConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            high_importance_boost: 1.4,
            importance_threshold: 6,  // Lowered from 7 to boost more results
        }
    }
}

/// Apply importance-based boosting to search results.
///
/// High-importance memories (importance >= threshold) get a score boost,
/// helping prioritize curated, high-value results over marginal matches.
pub fn boost_by_importance(
    results: &mut [SearchResult],
    config: &ImportanceBoostConfig,
) {
    if !config.enabled {
        return;
    }

    let mut boosted_count = 0;
    for result in results.iter_mut() {
        if result.importance >= config.importance_threshold {
            result.score *= config.high_importance_boost;
            boosted_count += 1;
            tracing::trace!(
                "Importance boost applied to {}: importance={}, new_score={:.4}",
                result.id,
                result.importance,
                result.score
            );
        }
    }

    if boosted_count > 0 {
        tracing::debug!(
            "Importance boosting applied to {} of {} results (threshold: {})",
            boosted_count,
            results.len(),
            config.importance_threshold
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_importance_boost_default_config() {
        let config = ImportanceBoostConfig::default();
        assert!(config.enabled);
        assert_eq!(config.high_importance_boost, 1.4);
        assert_eq!(config.importance_threshold, 6);
    }

    #[test]
    fn test_importance_boost_custom_config() {
        let config = ImportanceBoostConfig {
            enabled: true,
            high_importance_boost: 1.5,
            importance_threshold: 5,
        };
        assert_eq!(config.high_importance_boost, 1.5);
        assert_eq!(config.importance_threshold, 5);
    }

    #[test]
    fn test_importance_boosting_disabled() {
        let config = ImportanceBoostConfig {
            enabled: false,
            ..Default::default()
        };
        
        let mut results = vec![
            SearchResult {
                id: "test1".to_string(),
                memory_type: "test".to_string(),
                content: "test".to_string(),
                score: 1.0,
                importance: 10,
                tags: vec![],
                created_at: "".to_string(),
                source: "".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
            },
        ];
        let original_score = results[0].score;
        
        boost_by_importance(&mut results, &config);
        assert_eq!(results[0].score, original_score, "Score should not change when disabled");
    }

    #[test]
    fn test_importance_threshold_applied() {
        let config = ImportanceBoostConfig {
            enabled: true,
            high_importance_boost: 2.0,
            importance_threshold: 7,
        };
        
        let mut results = vec![
            SearchResult {
                id: "high".to_string(),
                memory_type: "test".to_string(),
                content: "test".to_string(),
                score: 1.0,
                importance: 9,
                tags: vec![],
                created_at: "".to_string(),
                source: "".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
            },
            SearchResult {
                id: "low".to_string(),
                memory_type: "test".to_string(),
                content: "test".to_string(),
                score: 1.0,
                importance: 5,
                tags: vec![],
                created_at: "".to_string(),
                source: "".to_string(),
                rel_type: None,
                direction: None,
                hop_depth: None,
                parent_id: None,
                quality_score: None,
            },
        ];
        
        boost_by_importance(&mut results, &config);
        assert_eq!(results[0].score, 2.0, "High importance should be boosted");
        assert_eq!(results[1].score, 1.0, "Low importance should not be boosted");
    }
}
