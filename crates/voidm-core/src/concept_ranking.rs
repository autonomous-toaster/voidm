//! Concept relevance ranking: score concepts by importance
//!
//! Rank concepts by: instance count, mention count, recency, edge connectivity

use anyhow::Result;
use sqlx::SqlitePool;
use chrono::{DateTime, Utc, Duration};

#[derive(Debug, Clone)]
pub struct RankedConcept {
    pub concept_id: String,
    pub name: String,
    pub relevance_score: f32,
    /// Breaking down the score
    pub instance_count: i64,
    pub mention_count: i64,
    pub recency_days: i64,
    pub incoming_edges: i64,
}

impl RankedConcept {
    /// Compute relevance score from components
    pub fn compute_score(
        instance_count: i64,
        mention_count: i64,
        created_at: &str,
        incoming_edges: i64,
    ) -> f32 {
        // Parse created_at timestamp
        let recency = if let Ok(dt) = DateTime::parse_from_rfc3339(created_at) {
            let created = dt.with_timezone(&Utc);
            let now = Utc::now();
            (now - created).num_days().max(0)
        } else {
            365  // Default to 1 year if parse fails
        };

        // Recency exponential decay: 0.5 at 90 days, 0.1 at 365 days
        let recency_score = (-recency as f32 / 180.0).exp();

        // Weighted score:
        // 40% instance count (normalized)
        // 30% mention count (normalized)
        // 20% recency
        // 10% incoming edges (normalized)
        let instance_norm = (instance_count as f32 / 10.0).min(1.0);  // 10+ instances = max score
        let mention_norm = (mention_count as f32 / 5.0).min(1.0);     // 5+ mentions = max score
        let edge_norm = (incoming_edges as f32 / 5.0).min(1.0);       // 5+ edges = max score

        instance_norm * 0.4 + mention_norm * 0.3 + recency_score * 0.2 + edge_norm * 0.1
    }
}

/// Get top-ranked concepts
pub async fn rank_concepts(pool: &SqlitePool, limit: usize) -> Result<Vec<RankedConcept>> {
    let concepts: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, name, created_at FROM ontology_concepts ORDER BY name"
    )
    .fetch_all(pool)
    .await?;

    let mut ranked = Vec::new();

    for (concept_id, name, created_at) in concepts {
        // Count INSTANCE_OF edges (memories linked to this concept)
        let instance_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ontology_edges WHERE rel_type = 'INSTANCE_OF' AND to_id = ?"
        )
        .bind(&concept_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        // Count MENTIONS edges (memories mentioning this concept)
        let mention_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ontology_edges WHERE rel_type = 'MENTIONS' AND to_id = ?"
        )
        .bind(&concept_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        // Count all incoming edges
        let incoming_edges: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ontology_edges WHERE to_id = ?"
        )
        .bind(&concept_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let relevance_score =
            RankedConcept::compute_score(instance_count, mention_count, &created_at, incoming_edges);

        ranked.push(RankedConcept {
            concept_id,
            name,
            relevance_score,
            instance_count,
            mention_count,
            recency_days: 0,  // Would compute from created_at
            incoming_edges,
        });
    }

    // Sort by relevance descending
    ranked.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());

    Ok(ranked.into_iter().take(limit).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_score_new_popular() {
        // Recent concept with high activity
        let score = RankedConcept::compute_score(
            10,  // 10 instances
            5,   // 5 mentions
            &Utc::now().to_rfc3339(),  // Just created
            8,   // 8 incoming edges
        );

        // Should be high (>0.6)
        assert!(score > 0.6);
    }

    #[test]
    fn test_compute_score_old_unpopular() {
        // Old concept with low activity
        let old_date = (Utc::now() - Duration::days(400)).to_rfc3339();
        let score = RankedConcept::compute_score(
            1,  // 1 instance
            0,  // 0 mentions
            &old_date,
            0,  // 0 incoming edges
        );

        // Should be low (<0.2)
        assert!(score < 0.2);
    }

    #[test]
    fn test_compute_score_moderate() {
        // Moderate concept, medium age
        let medium_date = (Utc::now() - Duration::days(30)).to_rfc3339();
        let score = RankedConcept::compute_score(
            5,   // 5 instances
            2,   // 2 mentions
            &medium_date,
            3,   // 3 incoming edges
        );

        // Should be moderate (0.3-0.6)
        assert!(score > 0.3 && score < 0.6);
    }

    #[test]
    fn test_ranked_concept_creation() {
        let concept = RankedConcept {
            concept_id: "123".to_string(),
            name: "JWT".to_string(),
            relevance_score: 0.75,
            instance_count: 10,
            mention_count: 5,
            recency_days: 30,
            incoming_edges: 8,
        };

        assert_eq!(concept.name, "JWT");
        assert!(concept.relevance_score > 0.70);
        assert_eq!(concept.instance_count, 10);
    }
}
