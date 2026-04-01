//! Recency-based score boosting for fresher, more timely results.
//!
//! Recent memories are prioritized to surface fresher content and ensure
//! that updates/corrections appear higher in search results.

use crate::search::SearchResult;
use std::time::{SystemTime, UNIX_EPOCH};

/// Recency boost configuration.
#[derive(Debug, Clone)]
pub struct RecencyBoostConfig {
    /// Enable recency boosting (default: true)
    pub enabled: bool,
    /// Boost for results updated within recency_days (default: 1.2)
    pub recent_boost: f32,
    /// Number of days to consider "recent" (default: 30)
    pub recency_days: u32,
}

impl Default for RecencyBoostConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            recent_boost: 1.3,  // Increased from 1.2 for stronger freshness preference
            recency_days: 30,
        }
    }
}

/// Apply recency-based boosting to search results.
///
/// Recent memories (updated within recency_days) get a score boost,
/// helping surface fresher content and corrections.
pub fn boost_by_recency(
    results: &mut [SearchResult],
    config: &RecencyBoostConfig,
) {
    if !config.enabled {
        return;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let recency_threshold = now - (config.recency_days as u64 * 86400); // 86400 seconds per day

    let mut boosted_count = 0;
    for result in results.iter_mut() {
        // Parse created_at timestamp (ISO 8601 format)
        if let Ok(updated) = parse_timestamp(&result.created_at) {
            if updated > recency_threshold {
                result.score *= config.recent_boost;
                boosted_count += 1;
                tracing::trace!(
                    "Recency boost applied to {}: created_at='{}', new_score={:.4}",
                    result.id,
                    result.created_at,
                    result.score
                );
            }
        }
    }

    if boosted_count > 0 {
        tracing::debug!(
            "Recency boosting applied to {} of {} results (within {} days)",
            boosted_count,
            results.len(),
            config.recency_days
        );
    }
}

/// Parse ISO 8601 timestamp to Unix seconds.
fn parse_timestamp(ts_str: &str) -> Result<u64, Box<dyn std::error::Error>> {
    // Simple ISO 8601 parser for format: "2026-03-24T12:34:56Z"
    use chrono::{DateTime, TimeZone};
    
    let dt = DateTime::parse_from_rfc3339(ts_str)
        .or_else(|_| {
            // Fallback: try parsing without timezone
            chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H:%M:%S")
                .map(|ndt| {
                    let dt = chrono::Utc.from_utc_datetime(&ndt);
                    dt.with_timezone(&chrono::FixedOffset::east_opt(0).unwrap())
                })
        })?;
    
    Ok(dt.timestamp() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recency_boost_default_config() {
        let config = RecencyBoostConfig::default();
        assert!(config.enabled);
        assert_eq!(config.recent_boost, 1.3);
        assert_eq!(config.recency_days, 30);
    }

    #[test]
    fn test_recency_boost_custom_config() {
        let config = RecencyBoostConfig {
            enabled: true,
            recent_boost: 1.5,
            recency_days: 7,
        };
        assert_eq!(config.recent_boost, 1.5);
        assert_eq!(config.recency_days, 7);
    }

    #[test]
    fn test_recency_boost_disabled() {
        let config = RecencyBoostConfig {
            enabled: false,
            ..Default::default()
        };
        
        let mut results = vec![SearchResult {
            id: "test1".to_string(),
            object_type: "memory".to_string(),
            memory_type: "test".to_string(),
            content: "test".to_string(),
            content_truncated: false,
            content_source: "memory_truncate".to_string(),
            context_chunks: Vec::new(),
            score: 1.0,
            importance: 5,
            tags: vec![],
            scopes: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            source: "".to_string(),
            rel_type: None,
            direction: None,
            hop_depth: None,
            parent_id: None,
            quality_score: None,
            title: None,
        }];
        
        let original_score = results[0].score;
        boost_by_recency(&mut results, &config);
        assert_eq!(results[0].score, original_score, "Score should not change when disabled");
    }

    #[test]
    fn test_parse_timestamp_rfc3339() {
        let ts = "2026-03-24T12:34:56Z";
        let result = parse_timestamp(ts);
        assert!(result.is_ok(), "Should parse RFC3339 timestamp");
    }
}
