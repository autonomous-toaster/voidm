//! Context-aware score boosting for improved precision and recall.
//!
//! When a query has explicit intent/context, boost results whose memory context
//! matches the query intent. This helps prioritize contextually relevant results.

use crate::search::SearchResult;

/// Context boost configuration.
#[derive(Debug, Clone)]
pub struct ContextBoostConfig {
    /// Enable context boosting (default: true)
    pub enabled: bool,
    /// Score multiplier for context-matching results (default: 1.3)
    pub context_match_boost: f32,
    /// Minimum context string length to consider (default: 3)
    pub min_context_length: usize,
}

impl Default for ContextBoostConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            context_match_boost: 1.3,
            min_context_length: 3,
        }
    }
}

/// Apply context-aware boosting to search results.
///
/// If query has intent/context, boost scores of results whose memory context
/// matches or overlaps with the query intent.
pub fn boost_by_context(
    results: &mut [SearchResult],
    query_intent: Option<&str>,
    config: &ContextBoostConfig,
) {
    if !config.enabled {
        return;
    }

    let Some(intent) = query_intent else {
        return;
    };

    // Parse intent into keywords
    let intent_keywords = extract_keywords(intent);
    if intent_keywords.is_empty() {
        return;
    }

    tracing::debug!(
        "Context boosting: applying to {} results with intent keywords: {:?}",
        results.len(),
        intent_keywords
    );

    for result in results.iter_mut() {
        // Check if result's memory_type overlaps with intent keywords
        if has_keyword_match(&result.memory_type, &intent_keywords) {
            result.score *= config.context_match_boost;
            tracing::trace!(
                "Context boost applied to {}: type='{}', new_score={:.4}",
                result.id,
                result.memory_type,
                result.score
            );
        }
    }
}

/// Extract searchable keywords from context string.
fn extract_keywords(context: &str) -> Vec<String> {
    context
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() >= 3)
        .map(|s| s.to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

/// Check if any keyword from keywords appears in text.
fn has_keyword_match(text: &str, keywords: &[String]) -> bool {
    let text_lower = text.to_lowercase();
    keywords.iter().any(|kw| text_lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_extraction() {
        let keywords = extract_keywords("database_optimization");
        assert!(keywords.contains(&"database".to_string()));
        assert!(keywords.contains(&"optimization".to_string()));
    }

    #[test]
    fn test_keyword_match() {
        let keywords = vec!["database".to_string(), "optimization".to_string()];
        assert!(has_keyword_match("database_performance", &keywords));
        assert!(has_keyword_match("optimization_techniques", &keywords));
        assert!(!has_keyword_match("memory_management", &keywords));
    }

    #[test]
    fn test_context_boosting_config() {
        let config = ContextBoostConfig::default();
        assert!(config.enabled);
        assert_eq!(config.context_match_boost, 1.3);
    }
}
