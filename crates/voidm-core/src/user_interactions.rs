//! User interaction tracking: record and analyze how users interact with voidm
//!
//! Track: searches, views, enrichments, feedback, merges, creations
//! Purpose: Understand user preferences and behavior patterns

use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct UserInteraction {
    pub user_id: String,           // agent_id or user_email
    pub interaction_type: String,  // search, view, enrich, feedback, merge, create
    pub target_id: String,         // concept_id or memory_id
    pub target_name: String,       // concept or memory name
    pub context: Option<String>,   // Additional context
    pub result: String,            // success, skip, cancel, error
    pub duration_ms: i64,          // How long did this take?
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct UserPreferences {
    pub user_id: String,
    pub favorite_scopes: Vec<(String, i64)>,      // (scope, count)
    pub favorite_concepts: Vec<(String, i64)>,    // (concept_name, views)
    pub enrichment_acceptance_rate: f32,          // 0.0-1.0
    pub preferred_concept_types: Vec<(String, f32)>, // (type, frequency)
    pub avg_interaction_duration_ms: i64,
    pub peak_activity_hours: Vec<u32>,            // 0-23
    pub typical_workflow: Vec<String>,            // [search, view, enrich, feedback]
}

#[derive(Debug, Clone)]
pub struct InteractionStats {
    pub user_id: String,
    pub total_interactions: i64,
    pub interaction_breakdown: Vec<(String, i64)>, // (type, count)
    pub success_rate: f32,
    pub avg_duration_ms: i64,
    pub last_interaction: chrono::DateTime<chrono::Utc>,
    pub active_since: chrono::DateTime<chrono::Utc>,
}

/// Track a user interaction
pub async fn track_interaction(
    pool: &SqlitePool,
    user_id: &str,
    interaction_type: &str,
    target_id: &str,
    target_name: &str,
    result: &str,
    duration_ms: i64,
    context: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO user_interactions 
         (user_id, interaction_type, target_id, target_name, result, duration_ms, timestamp, context)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(user_id)
    .bind(interaction_type)
    .bind(target_id)
    .bind(target_name)
    .bind(result)
    .bind(duration_ms)
    .bind(Utc::now())
    .bind(context)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get user statistics
pub async fn get_user_stats(pool: &SqlitePool, user_id: &str) -> Result<Option<InteractionStats>> {
    let total: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_interactions WHERE user_id = ?"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    if total.is_none() || total == Some(0) {
        return Ok(None);
    }

    let breakdown: Vec<(String, i64)> = sqlx::query_as(
        "SELECT interaction_type, COUNT(*) as count
         FROM user_interactions
         WHERE user_id = ?
         GROUP BY interaction_type
         ORDER BY count DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let success_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_interactions WHERE user_id = ? AND result = 'success'"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let success_rate = if let Some(total_val) = total {
        (success_count as f32) / (total_val as f32)
    } else {
        0.0
    };

    let avg_duration: Option<i64> = sqlx::query_scalar(
        "SELECT AVG(duration_ms) FROM user_interactions WHERE user_id = ?"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let last_interaction: Option<String> = sqlx::query_scalar(
        "SELECT MAX(timestamp) FROM user_interactions WHERE user_id = ?"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let first_interaction: Option<String> = sqlx::query_scalar(
        "SELECT MIN(timestamp) FROM user_interactions WHERE user_id = ?"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let last_dt = last_interaction
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let first_dt = first_interaction
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    Ok(Some(InteractionStats {
        user_id: user_id.to_string(),
        total_interactions: total.unwrap_or(0),
        interaction_breakdown: breakdown,
        success_rate,
        avg_duration_ms: avg_duration.unwrap_or(0),
        last_interaction: last_dt,
        active_since: first_dt,
    }))
}

/// Infer user preferences from interactions
pub async fn infer_user_preferences(pool: &SqlitePool, user_id: &str) -> Result<Option<UserPreferences>> {
    let stats = get_user_stats(pool, user_id).await?;

    if stats.is_none() {
        return Ok(None);
    }

    // Get favorite concepts
    let favorite_concepts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT target_name, COUNT(*) as count
         FROM user_interactions
         WHERE user_id = ? AND interaction_type IN ('search', 'view')
         GROUP BY target_name
         ORDER BY count DESC
         LIMIT 10"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    // Get preferred scopes (from context or target analysis)
    let favorite_scopes: Vec<(String, i64)> = vec![];  // TODO: parse from context or memory analysis

    // Calculate enrichment acceptance rate
    let enrichment_total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_interactions WHERE user_id = ? AND interaction_type = 'enrich'"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let enrichment_accepted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_interactions WHERE user_id = ? AND interaction_type = 'enrich' AND result = 'success'"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let enrichment_acceptance_rate = if enrichment_total > 0 {
        (enrichment_accepted as f32) / (enrichment_total as f32)
    } else {
        0.5  // Default neutral
    };

    // Get peak activity hours
    let peak_hours: Vec<u32> = sqlx::query_scalar(
        "SELECT CAST(strftime('%H', timestamp) AS INTEGER) as hour
         FROM user_interactions
         WHERE user_id = ?
         GROUP BY hour
         ORDER BY COUNT(*) DESC
         LIMIT 3"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Ok(Some(UserPreferences {
        user_id: user_id.to_string(),
        favorite_scopes,
        favorite_concepts,
        enrichment_acceptance_rate,
        preferred_concept_types: vec![],  // TODO: infer from searches
        avg_interaction_duration_ms: stats.as_ref().map(|s| s.avg_duration_ms).unwrap_or(0),
        peak_activity_hours: peak_hours,
        typical_workflow: vec![],  // TODO: sequence analysis
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_interaction_creation() {
        let interaction = UserInteraction {
            user_id: "user-123".to_string(),
            interaction_type: "search".to_string(),
            target_id: "concept-456".to_string(),
            target_name: "JWT".to_string(),
            context: Some("search:authentication".to_string()),
            result: "success".to_string(),
            duration_ms: 250,
            timestamp: Utc::now(),
        };

        assert_eq!(interaction.user_id, "user-123");
        assert_eq!(interaction.interaction_type, "search");
        assert!(interaction.duration_ms > 0);
    }

    #[test]
    fn test_user_preferences_creation() {
        let prefs = UserPreferences {
            user_id: "user-123".to_string(),
            favorite_scopes: vec![
                ("auth".to_string(), 15),
                ("performance".to_string(), 8),
            ],
            favorite_concepts: vec![
                ("JWT".to_string(), 10),
                ("REST".to_string(), 7),
            ],
            enrichment_acceptance_rate: 0.85,
            preferred_concept_types: vec![
                ("TECHNIQUE".to_string(), 0.6),
                ("PATTERN".to_string(), 0.3),
            ],
            avg_interaction_duration_ms: 500,
            peak_activity_hours: vec![9, 14, 18],
            typical_workflow: vec!["search".to_string(), "view".to_string(), "enrich".to_string()],
        };

        assert_eq!(prefs.enrichment_acceptance_rate, 0.85);
        assert!(prefs.favorite_concepts[0].1 > prefs.favorite_concepts[1].1);
    }

    #[test]
    fn test_interaction_stats() {
        let stats = InteractionStats {
            user_id: "user-123".to_string(),
            total_interactions: 150,
            interaction_breakdown: vec![
                ("search".to_string(), 80),
                ("view".to_string(), 50),
                ("enrich".to_string(), 20),
            ],
            success_rate: 0.92,
            avg_duration_ms: 350,
            last_interaction: Utc::now(),
            active_since: Utc::now(),
        };

        assert_eq!(stats.total_interactions, 150);
        assert!(stats.success_rate > 0.9);
    }
}
