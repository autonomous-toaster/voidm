//! Concept telemetry and usage tracking
//!
//! Track how agents use concepts: searches, retrievals, edge traversals
//! Power the self-improvement feedback loop

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct ConceptTelemetry {
    pub concept_id: String,
    pub concept_name: String,
    pub event_type: String,     // "query", "instance_fetch", "edge_traverse", "feedback"
    pub timestamp: DateTime<Utc>,
    pub agent_id: Option<String>, // Which agent (e.g., "pi", "claude", "user")
    pub context: Option<String>,  // What was the agent doing? e.g., "search:auth", "enrich:JWT"
}

#[derive(Debug, Clone)]
pub struct ConceptUsageStats {
    pub concept_id: String,
    pub concept_name: String,
    pub total_queries: i64,       // How many times agents searched for this
    pub total_retrievals: i64,    // How many times this concept was returned
    pub total_edges_traversed: i64, // How many times agents followed edges from this
    pub positive_feedback: i64,   // Agent said "helpful"
    pub negative_feedback: i64,   // Agent said "not helpful" or "duplicate"
    pub last_used: DateTime<Utc>,
    pub quality_score: f32,       // Computed from: retrievals/queries, feedback ratio
}

/// Record a concept usage event
pub async fn track_concept_event(
    pool: &SqlitePool,
    concept_id: &str,
    concept_name: &str,
    event_type: &str,
    agent_id: Option<&str>,
    context: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO concept_telemetry (concept_id, concept_name, event_type, timestamp, agent_id, context)
         VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(concept_id)
    .bind(concept_name)
    .bind(event_type)
    .bind(Utc::now())
    .bind(agent_id)
    .bind(context)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get usage statistics for a concept
pub async fn get_concept_stats(
    pool: &SqlitePool,
    concept_id: &str,
) -> Result<Option<ConceptUsageStats>> {
    // Query aggregate counts from telemetry
    let row: Option<(String, i64, i64, i64, String)> = sqlx::query_as(
        "SELECT concept_name,
                COUNT(CASE WHEN event_type = 'query' THEN 1 END) as queries,
                COUNT(CASE WHEN event_type = 'instance_fetch' THEN 1 END) as retrievals,
                COUNT(CASE WHEN event_type = 'edge_traverse' THEN 1 END) as edges,
                MAX(timestamp) as last_used
         FROM concept_telemetry
         WHERE concept_id = ?
         GROUP BY concept_id"
    )
    .bind(concept_id)
    .fetch_optional(pool)
    .await?;

    if let Some((name, queries, retrievals, edges, last_used)) = row {
        // Get feedback stats separately
        let positive: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_feedback WHERE concept_id = ? AND feedback_type = 'helpful'"
        )
        .bind(concept_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);

        let negative: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_feedback WHERE concept_id = ? AND feedback_type != 'helpful'"
        )
        .bind(concept_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);

        let last_used_dt = DateTime::parse_from_rfc3339(&last_used)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        // Quality score: (retrievals + edges) / (queries + 1), adjusted by feedback
        let utilization = if queries > 0 {
            ((retrievals + edges) as f32) / (queries as f32 + 1.0)
        } else {
            0.0
        };

        let feedback_sentiment = if positive + negative > 0 {
            (positive as f32) / (positive + negative) as f32
        } else {
            0.5  // Neutral if no feedback
        };

        let quality_score = (utilization * 0.6 + feedback_sentiment * 0.4).min(1.0);

        return Ok(Some(ConceptUsageStats {
            concept_id: concept_id.to_string(),
            concept_name: name,
            total_queries: queries,
            total_retrievals: retrievals,
            total_edges_traversed: edges,
            positive_feedback: positive,
            negative_feedback: negative,
            last_used: last_used_dt,
            quality_score,
        }));
    }

    Ok(None)
}

/// Find low-quality concepts (candidates for merging or removal)
pub async fn find_low_quality_concepts(pool: &SqlitePool, quality_threshold: f32) -> Result<Vec<ConceptUsageStats>> {
    let mut stats = Vec::new();

    let concepts: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT concept_id FROM concept_telemetry"
    )
    .fetch_all(pool)
    .await?;

    for concept_id in concepts {
        if let Some(stat) = get_concept_stats(pool, &concept_id).await? {
            if stat.quality_score < quality_threshold {
                stats.push(stat);
            }
        }
    }

    stats.sort_by(|a, b| a.quality_score.partial_cmp(&b.quality_score).unwrap());
    Ok(stats)
}

/// Find high-opportunity concepts (frequently queried but not extracted)
pub async fn find_missing_concepts(pool: &SqlitePool, query_threshold: i64) -> Result<Vec<(String, i64)>> {
    // Find queries that didn't result in retrievals
    let missing: Vec<(String, i64)> = sqlx::query_as(
        "SELECT context, COUNT(*) as count
         FROM concept_telemetry
         WHERE event_type = 'query'
         GROUP BY context
         HAVING COUNT(*) >= ?
         ORDER BY COUNT(*) DESC
         LIMIT 20"
    )
    .bind(query_threshold)
    .fetch_all(pool)
    .await?;

    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concept_telemetry_creation() {
        let telemetry = ConceptTelemetry {
            concept_id: "123".to_string(),
            concept_name: "JWT".to_string(),
            event_type: "query".to_string(),
            timestamp: Utc::now(),
            agent_id: Some("pi".to_string()),
            context: Some("search:authentication".to_string()),
        };

        assert_eq!(telemetry.concept_name, "JWT");
        assert_eq!(telemetry.event_type, "query");
    }

    #[test]
    fn test_quality_score_calculation() {
        // High retrieval rate, positive feedback
        let stats = ConceptUsageStats {
            concept_id: "123".to_string(),
            concept_name: "JWT".to_string(),
            total_queries: 10,
            total_retrievals: 8,
            total_edges_traversed: 5,
            positive_feedback: 10,
            negative_feedback: 0,
            last_used: Utc::now(),
            quality_score: 0.0,  // Will compute: (13/11) * 0.6 + 1.0 * 0.4 = 1.0 (capped)
        };

        let utilization = (stats.total_retrievals + stats.total_edges_traversed) as f32
            / (stats.total_queries as f32 + 1.0);
        let feedback_sentiment = stats.positive_feedback as f32 / (stats.positive_feedback + stats.negative_feedback) as f32;
        let quality = (utilization * 0.6 + feedback_sentiment * 0.4).min(1.0);

        assert!(quality > 0.8);
    }

    #[test]
    fn test_low_quality_concept() {
        // Low usage, negative feedback
        let stats = ConceptUsageStats {
            concept_id: "456".to_string(),
            concept_name: "unused_thing".to_string(),
            total_queries: 1,
            total_retrievals: 0,
            total_edges_traversed: 0,
            positive_feedback: 0,
            negative_feedback: 2,
            last_used: Utc::now(),
            quality_score: 0.0,
        };

        let utilization = (stats.total_retrievals + stats.total_edges_traversed) as f32
            / (stats.total_queries as f32 + 1.0);
        let feedback_sentiment = if stats.positive_feedback + stats.negative_feedback > 0 {
            stats.positive_feedback as f32 / (stats.positive_feedback + stats.negative_feedback) as f32
        } else {
            0.5
        };
        let quality = (utilization * 0.6 + feedback_sentiment * 0.4).min(1.0);

        assert!(quality < 0.3);
    }
}
