//! User-Aware Adaptive System: Integrates user behavior into all voidm operations
//!
//! Makes the system learn user preferences and adapt automatically

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct UserAdaptiveConfig {
    /// Track user interactions automatically?
    pub auto_track_interactions: bool,
    /// Apply personalized rankings?
    pub personalize_rankings: bool,
    /// Use smart defaults based on user history?
    pub adaptive_defaults: bool,
    /// Predict and suggest next steps?
    pub predict_next_steps: bool,
    /// Learn from user acceptance of suggestions?
    pub learn_from_feedback: bool,
    /// Minimum interactions before personalizing (avoid cold start)
    pub personalization_threshold: i64,
}

impl Default for UserAdaptiveConfig {
    fn default() -> Self {
        UserAdaptiveConfig {
            auto_track_interactions: true,
            personalize_rankings: true,
            adaptive_defaults: true,
            predict_next_steps: true,
            learn_from_feedback: true,
            personalization_threshold: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserAdaptiveResponse {
    pub user_id: String,
    pub suggestions: Vec<String>,          // Personalized suggestions
    pub recommended_next_step: Option<String>,  // What to do next
    pub defaults: Option<crate::user_personalization::UserDefaults>,
    pub work_style_detected: Option<String>,
    pub confidence: f32,                   // How confident is the adaptation (0-1)?
}

/// Check if user has enough history for personalization
pub async fn should_personalize(
    pool: &SqlitePool,
    user_id: &str,
    threshold: i64,
) -> Result<bool> {
    let interaction_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_interactions WHERE user_id = ?"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    Ok(interaction_count.unwrap_or(0) >= threshold)
}

/// Get adaptive recommendations for a user
pub async fn get_adaptive_response(
    pool: &SqlitePool,
    user_id: &str,
    config: &UserAdaptiveConfig,
) -> Result<UserAdaptiveResponse> {
    let mut suggestions = Vec::new();
    let mut recommended_next_step = None;
    let mut defaults = None;
    let mut work_style_detected = None;
    let mut confidence = 0.0;

    // Check if user has enough history
    let has_history = should_personalize(pool, user_id, config.personalization_threshold).await?;

    if !has_history {
        // Return generic defaults for new users
        return Ok(UserAdaptiveResponse {
            user_id: user_id.to_string(),
            suggestions,
            recommended_next_step,
            defaults,
            work_style_detected,
            confidence: 0.0_f32,  // No confidence yet
        });
    }

    // User has enough history - apply personalization
    let mut confidence: f32 = 0.75_f32;  // Reasonable confidence with 10+ interactions

    // 1. Get personalized concept rankings
    if config.personalize_rankings {
        let rankings = crate::user_personalization::get_personalized_rankings(pool, user_id, 5)
            .await
            .unwrap_or_default();

        suggestions = rankings.into_iter()
            .map(|r| format!("{} ({:.0}%)", r.concept_name, r.personalized_score * 100.0))
            .collect();
    }

    // 2. Predict next step
    if config.predict_next_steps {
        if let Ok(Some(prediction)) = crate::user_personalization::predict_next_step(pool, user_id, None).await {
            recommended_next_step = Some(format!(
                "Next: {} {}  ({})",
                prediction.action, prediction.target, prediction.reason
            ));
            confidence = confidence.max(prediction.confidence);
        }
    }

    // 3. Get smart defaults
    if config.adaptive_defaults {
        if let Ok(Some(user_defaults)) = crate::user_personalization::infer_user_defaults(pool, user_id).await {
            defaults = Some(user_defaults);
        }
    }

    // 4. Detect work style
    if let Ok(Some(work_style)) = crate::user_workflow::infer_work_style(pool, user_id).await {
        work_style_detected = Some(format!(
            "{} ({})",
            work_style.primary_pattern.pattern_name,
            if work_style.is_explorer { "explorer" } else if work_style.is_focused { "focused" } else { "balanced" }
        ));
        confidence = confidence.max(work_style.primary_pattern.confidence);
    }

    Ok(UserAdaptiveResponse {
        user_id: user_id.to_string(),
        suggestions,
        recommended_next_step,
        defaults,
        work_style_detected,
        confidence: confidence.min(1.0_f32),
    })
}

/// Track an interaction and update user profile
pub async fn track_and_learn(
    pool: &SqlitePool,
    user_id: &str,
    interaction_type: &str,
    target_id: &str,
    target_name: &str,
    result: &str,
    duration_ms: i64,
    context: Option<&str>,
) -> Result<()> {
    // Record the interaction
    crate::user_interactions::track_interaction(
        pool,
        user_id,
        interaction_type,
        target_id,
        target_name,
        result,
        duration_ms,
        context,
    )
    .await?;

    // TODO: Trigger async profile update if interaction_count % 5 == 0
    // (Recompute user_preferences table every 5 interactions)

    Ok(())
}

/// Personalize memory enrichment for a user
pub async fn personalize_enrichment(
    pool: &SqlitePool,
    user_id: &str,
    _memory_id: &str,
) -> Result<EnrichmentPersonalization> {
    // Get user preferences
    let prefs = crate::user_interactions::infer_user_preferences(pool, user_id).await?;

    if prefs.is_none() {
        return Ok(EnrichmentPersonalization::default());
    }

    let prefs_val = prefs.unwrap();

    // Customize enrichment based on user preferences
    let enrichment_depth = if prefs_val.enrichment_acceptance_rate > 0.8 {
        "comprehensive"  // They like detailed enrichment
    } else {
        "minimal"        // They like simple enrichment
    };

    let suggestion_count = if prefs_val.favorite_concepts.len() > 10 {
        10  // Active user, show more options
    } else {
        3   // Casual user, keep it simple
    };

    Ok(EnrichmentPersonalization {
        enrichment_depth: enrichment_depth.to_string(),
        suggestion_count,
        preferred_scope: prefs_val.favorite_scopes.first().map(|(s, _)| s.clone()),
        preferred_type: prefs_val.preferred_concept_types.first().map(|(t, _)| t.clone()),
        auto_accept_threshold: prefs_val.enrichment_acceptance_rate,
    })
}

#[derive(Debug, Clone)]
pub struct EnrichmentPersonalization {
    pub enrichment_depth: String,     // "minimal", "balanced", "comprehensive"
    pub suggestion_count: i32,        // How many options to suggest
    pub preferred_scope: Option<String>,
    pub preferred_type: Option<String>,
    pub auto_accept_threshold: f32,   // Accept suggestions with score > this
}

impl Default for EnrichmentPersonalization {
    fn default() -> Self {
        EnrichmentPersonalization {
            enrichment_depth: "balanced".to_string(),
            suggestion_count: 5,
            preferred_scope: None,
            preferred_type: None,
            auto_accept_threshold: 0.75,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_config_default() {
        let config = UserAdaptiveConfig::default();
        assert!(config.auto_track_interactions);
        assert!(config.personalize_rankings);
        assert_eq!(config.personalization_threshold, 10);
    }

    #[test]
    fn test_adaptive_response_creation() {
        let response = UserAdaptiveResponse {
            user_id: "user-123".to_string(),
            suggestions: vec!["JWT (92%)".to_string(), "REST (85%)".to_string()],
            recommended_next_step: Some("Next: enrich authentication".to_string()),
            defaults: None,
            work_style_detected: Some("deep-dive (explorer)".to_string()),
            confidence: 0.82,
        };

        assert_eq!(response.suggestions.len(), 2);
        assert!(response.confidence > 0.8);
        assert!(response.recommended_next_step.is_some());
    }

    #[test]
    fn test_enrichment_personalization() {
        let pers = EnrichmentPersonalization {
            enrichment_depth: "comprehensive".to_string(),
            suggestion_count: 10,
            preferred_scope: Some("security".to_string()),
            preferred_type: Some("TECHNIQUE".to_string()),
            auto_accept_threshold: 0.85,
        };

        assert_eq!(pers.enrichment_depth, "comprehensive");
        assert_eq!(pers.suggestion_count, 10);
        assert!(pers.auto_accept_threshold > 0.80);
    }
}
