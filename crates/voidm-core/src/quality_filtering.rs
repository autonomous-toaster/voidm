//! Quality-based result filtering for better precision and reliability.
//!
//! Filters out low-quality memories to improve result reliability and precision,
//! ensuring only curated content reaches users.

use crate::search::SearchResult;

/// Quality filtering configuration.
#[derive(Debug, Clone)]
pub struct QualityFilterConfig {
    /// Enable quality filtering (default: true)
    pub enabled: bool,
    /// Minimum quality score (0.0-1.0) to include results (default: 0.5)
    pub min_quality_score: f32,
    /// If true, results without quality_score are always included (default: true)
    pub include_unscored: bool,
}

impl Default for QualityFilterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_quality_score: 0.5,  // Increased from 0.4 for stricter quality filtering
            include_unscored: true,
        }
    }
}

/// Apply quality-based filtering to search results.
///
/// Removes results with quality_score < threshold, improving result reliability.
/// Results without a quality_score are preserved by default (include_unscored=true).
pub fn filter_by_quality(
    results: &mut Vec<SearchResult>,
    config: &QualityFilterConfig,
) -> usize {
    if !config.enabled {
        return 0;
    }

    let original_count = results.len();
    
    results.retain(|result| {
        if let Some(quality) = result.quality_score {
            quality >= config.min_quality_score
        } else {
            config.include_unscored
        }
    });
    
    let filtered_count = original_count - results.len();
    
    if filtered_count > 0 {
        tracing::debug!(
            "Quality filtering removed {} of {} results (threshold: {:.2})",
            filtered_count,
            original_count,
            config.min_quality_score
        );
    }
    
    filtered_count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_result(id: &str, quality: Option<f32>) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            memory_type: "test".to_string(),
            content: "test".to_string(),
            score: 1.0,
            importance: 5,
            tags: vec![],
            scopes: vec![],
            created_at: "".to_string(),
            source: "".to_string(),
            rel_type: None,
            direction: None,
            hop_depth: None,
            parent_id: None,
            quality_score: quality,
            title: None,
        }
    }

    #[test]
    fn test_quality_filter_default_config() {
        let config = QualityFilterConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_quality_score, 0.5);
        assert!(config.include_unscored);
    }

    #[test]
    fn test_quality_filter_disabled() {
        let config = QualityFilterConfig {
            enabled: false,
            ..Default::default()
        };
        
        let mut results = vec![
            create_test_result("low", Some(0.2)),
            create_test_result("high", Some(0.8)),
        ];
        let original_count = results.len();
        
        let filtered = filter_by_quality(&mut results, &config);
        assert_eq!(results.len(), original_count, "No filtering when disabled");
        assert_eq!(filtered, 0);
    }

    #[test]
    fn test_quality_filter_removes_low_quality() {
        let config = QualityFilterConfig {
            enabled: true,
            min_quality_score: 0.5,
            include_unscored: false,
        };
        
        let mut results = vec![
            create_test_result("low", Some(0.2)),
            create_test_result("high", Some(0.8)),
            create_test_result("unscored", None),
        ];
        
        let filtered = filter_by_quality(&mut results, &config);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "high");
        assert_eq!(filtered, 2);
    }

    #[test]
    fn test_quality_filter_includes_unscored() {
        let config = QualityFilterConfig {
            enabled: true,
            min_quality_score: 0.5,
            include_unscored: true,
        };
        
        let mut results = vec![
            create_test_result("low", Some(0.2)),
            create_test_result("high", Some(0.8)),
            create_test_result("unscored", None),
        ];
        
        let filtered = filter_by_quality(&mut results, &config);
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.id == "high"));
        assert!(results.iter().any(|r| r.id == "unscored"));
        assert_eq!(filtered, 1);
    }

    #[test]
    fn test_quality_filter_threshold() {
        let config = QualityFilterConfig {
            enabled: true,
            min_quality_score: 0.6,
            include_unscored: false,
        };
        
        let mut results = vec![
            create_test_result("a", Some(0.3)),
            create_test_result("b", Some(0.5)),
            create_test_result("c", Some(0.6)),
            create_test_result("d", Some(0.9)),
        ];
        
        let filtered = filter_by_quality(&mut results, &config);
        assert_eq!(results.len(), 2);
        assert_eq!(filtered, 2);
        assert!(results.iter().any(|r| r.id == "c"));
        assert!(results.iter().any(|r| r.id == "d"));
    }
}
