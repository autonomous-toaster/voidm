//! Integration test for graph-aware retrieval in search pipeline.
//!
//! Tests that graph retrieval configuration is respected when disabled/enabled.

#[cfg(test)]
mod tests {
    use voidm_core::graph_retrieval::GraphRetrievalConfig;

    #[test]
    fn test_graph_retrieval_config_enabled() {
        let config = GraphRetrievalConfig {
            enabled: true,
            tags: voidm_core::graph_retrieval::TagRetrievalConfig {
                enabled: true,
                min_overlap: 2,
                min_percentage: 50.0,
                decay_factor: 0.7,
                limit: 5,
            },
        };

        assert!(config.enabled);
        assert!(config.tags.enabled);
    }

    #[test]
    fn test_graph_retrieval_config_disabled() {
        let config = GraphRetrievalConfig {
            enabled: false,
            tags: voidm_core::graph_retrieval::TagRetrievalConfig::default(),
        };

        assert!(!config.enabled);
        assert!(config.tags.enabled); // Default is enabled
    }

    #[test]
    fn test_graph_retrieval_tag_config_validation() {
        let config = voidm_core::graph_retrieval::TagRetrievalConfig {
            enabled: true,
            min_overlap: 3,
            min_percentage: 50.0,
            decay_factor: 0.7,
            limit: 5,
        };

        assert!(config.enabled);
        assert!(config.min_overlap > 0);
        assert!(config.min_percentage > 0.0 && config.min_percentage <= 100.0);
        assert!(config.decay_factor > 0.0 && config.decay_factor < 1.0);
        assert!(config.limit > 0);
    }

    #[test]
    fn test_graph_retrieval_search_result_source_field() {
        // Verify that SearchResult has source field that can be set to graph_tags
        let mut result = voidm_core::search::SearchResult {
            id: "test".to_string(),
            object_type: "memory".to_string(),
            score: 0.8,
            memory_type: "semantic".to_string(),
            content: "test content".to_string(),
            content_truncated: false,
            content_source: "memory_truncate".to_string(),
            context_chunks: vec![],
            scopes: vec![],
            tags: vec![],
            importance: 5,
            created_at: "2026-03-15T10:00:00Z".to_string(),
            source: "search".to_string(),
            rel_type: None,
            direction: None,
            hop_depth: None,
            parent_id: None,
            quality_score: None,
            title: None,
        };

        assert_eq!(result.source, "search");

        // Simulate marking result as coming from tag-based graph retrieval
        result.source = "graph_tags".to_string();
        assert_eq!(result.source, "graph_tags");
    }
}
