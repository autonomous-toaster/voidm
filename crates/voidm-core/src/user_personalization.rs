//! Adaptive personalization: customize voidm based on user preferences and behavior
//!
//! Adapt: concept rankings, suggestion order, defaults, UI complexity
//! Predict: next steps, useful concepts, likely searches

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct PersonalizedRanking {
    pub concept_id: String,
    pub concept_name: String,
    pub base_relevance_score: f32,      // From concept_ranking module
    pub user_affinity_score: f32,       // How much user likes this type/domain
    pub personalized_score: f32,        // Weighted combination
    pub boost_reason: Option<String>,   // Why boosted? "matches past search", "expert domain"
}

#[derive(Debug, Clone)]
pub struct PredictedNextStep {
    pub action: String,                 // "enrich", "search", "create", "explore"
    pub target: String,                 // Concept/memory name
    pub confidence: f32,                // 0-1
    pub reason: String,                 // "User searches this every Mon", "Users like you explore X after Y"
}

#[derive(Debug, Clone)]
pub struct UserDefaults {
    pub preferred_scope: Option<String>,  // Default scope for new memories
    pub preferred_concept_type: Option<String>,  // Prefer PATTERN over TECHNIQUE?
    pub preferred_enrichment_depth: String,  // "minimal", "balanced", "comprehensive"
    pub preferred_suggestion_count: i32,  // How many options at once?
    pub auto_accept_confidence: f32,    // Accept suggestions if score > this?
    pub verbosity: String,              // "terse", "normal", "detailed"
}

/// Get personalized concept rankings for a user
pub async fn get_personalized_rankings(
    pool: &SqlitePool,
    user_id: &str,
    limit: usize,
) -> Result<Vec<PersonalizedRanking>> {
    // Get user preferences
    let user_prefs = crate::user_interactions::infer_user_preferences(pool, user_id).await?;

    if user_prefs.is_none() {
        return Ok(Vec::new());
    }

    let prefs = user_prefs.unwrap();

    // Get top concepts by base relevance
    let concepts: Vec<(String, String, f32)> = sqlx::query_as(
        "SELECT c.id, c.name, 
                (
                  COUNT(CASE WHEN e.rel_type = 'INSTANCE_OF' THEN 1 END) * 0.4 +
                  COUNT(CASE WHEN e.rel_type = 'MENTIONS' THEN 1 END) * 0.3 +
                  0.2 +  -- recency
                  COUNT(CASE WHEN e.rel_type != 'INSTANCE_OF' AND e.rel_type != 'MENTIONS' THEN 1 END) * 0.1
                ) as base_score
         FROM ontology_concepts c
         LEFT JOIN ontology_edges e ON c.id = e.to_id
         GROUP BY c.id
         ORDER BY base_score DESC
         LIMIT ?"
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let mut rankings = Vec::new();

    for (concept_id, concept_name, base_score) in concepts {
        // Calculate user affinity: how much does user like this concept/type/domain?
        let affinity = if prefs.favorite_concepts.iter().any(|(name, _)| name == &concept_name) 
                         && prefs.avg_concept_quality > 0.7 {
            // Only boost concepts if user's overall concept quality is good
            0.9
        } else {
            0.5  // Neutral if not in favorites OR user has poor concept quality
        };

        let personalized_score = (base_score * 0.6 + affinity * 0.4).min(1.0);

        rankings.push(PersonalizedRanking {
            concept_id,
            concept_name,
            base_relevance_score: base_score,
            user_affinity_score: affinity,
            personalized_score,
            boost_reason: if affinity > 0.7 {
                Some("Matches your interests".to_string())
            } else {
                None
            },
        });
    }

    // Sort by personalized score
    rankings.sort_by(|a, b| b.personalized_score.partial_cmp(&a.personalized_score).unwrap());

    Ok(rankings)
}

/// Predict user's next step based on patterns
pub async fn predict_next_step(
    pool: &SqlitePool,
    user_id: &str,
    _last_interaction: Option<&str>,
) -> Result<Option<PredictedNextStep>> {
    // Get user work style
    let work_style = crate::user_workflow::infer_work_style(pool, user_id).await?;

    if work_style.is_none() {
        return Ok(None);
    }

    let style = work_style.unwrap();

    // Based on primary pattern, predict next step
    let next_action = match style.primary_pattern.pattern_name.as_str() {
        "quick-search" => "search",      // Users do quick searches repeatedly
        "deep-dive" => "enrich",         // Explorers like to enrich
        "enrichment" => "feedback",      // Enrichers give feedback
        "collaborative" => "merge",      // Collaborators merge concepts
        "batch" => "create",             // Batch users create often
        _ => "view",                     // Default to view
    };

    // Find most likely target (concept they'd use next)
    let likely_target: Option<String> = sqlx::query_scalar(
        "SELECT target_name FROM user_interactions
         WHERE user_id = ? AND interaction_type = ?
         GROUP BY target_name
         ORDER BY COUNT(*) DESC
         LIMIT 1"
    )
    .bind(user_id)
    .bind(next_action)
    .fetch_optional(pool)
    .await?
    .flatten();

    if let Some(target) = likely_target {
        return Ok(Some(PredictedNextStep {
            action: next_action.to_string(),
            target: target.clone(),
            confidence: 0.72,
            reason: format!("Based on your {} patterns", style.primary_pattern.pattern_name),
        }));
    }

    Ok(None)
}

/// Get recommended defaults based on user behavior
pub async fn infer_user_defaults(pool: &SqlitePool, user_id: &str) -> Result<Option<UserDefaults>> {
    let prefs = crate::user_interactions::infer_user_preferences(pool, user_id).await?;
    let work_style = crate::user_workflow::infer_work_style(pool, user_id).await?;

    if prefs.is_none() || work_style.is_none() {
        return Ok(None);
    }

    let prefs_val = prefs.unwrap();
    let style_val = work_style.unwrap();

    // Preferred scope: first element of favorite_scopes
    let preferred_scope = prefs_val.favorite_scopes.first().map(|(scope, _)| scope.clone());

    // Preferred concept type: most common in searches
    let preferred_concept_type = prefs_val.preferred_concept_types.first()
        .map(|(type_name, _)| type_name.clone());

    // Enrichment depth based on work style
    let enrichment_depth = if style_val.is_explorer {
        "comprehensive"
    } else if style_val.primary_pattern.avg_cycle_time_ms > 1500 {
        "balanced"
    } else {
        "minimal"
    };

    // Suggestion count based on pace
    let suggestion_count = match style_val.preferred_pace.as_str() {
        "quick" => 3,    // Quick users want fewer options
        "normal" => 5,   // Normal users want moderate options
        "deep" => 10,    // Deep-dive users want more options
        _ => 5,
    };

    // Auto-accept confidence: use user's enrichment acceptance rate
    let auto_accept_confidence = prefs_val.enrichment_acceptance_rate;

    // Verbosity based on interaction duration
    let verbosity = if style_val.primary_pattern.avg_cycle_time_ms < 500 {
        "terse"
    } else if style_val.primary_pattern.avg_cycle_time_ms > 2000 {
        "detailed"
    } else {
        "normal"
    };

    Ok(Some(UserDefaults {
        preferred_scope,
        preferred_concept_type,
        preferred_enrichment_depth: enrichment_depth.to_string(),
        preferred_suggestion_count: suggestion_count,
        auto_accept_confidence,
        verbosity: verbosity.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_personalized_ranking_creation() {
        let ranking = PersonalizedRanking {
            concept_id: "jwt-id".to_string(),
            concept_name: "JWT".to_string(),
            base_relevance_score: 0.85,
            user_affinity_score: 0.95,
            personalized_score: 0.90,
            boost_reason: Some("Matches your interests".to_string()),
        };

        assert_eq!(ranking.concept_name, "JWT");
        assert!(ranking.personalized_score > ranking.base_relevance_score);
    }

    #[test]
    fn test_predicted_next_step() {
        let prediction = PredictedNextStep {
            action: "enrich".to_string(),
            target: "authentication".to_string(),
            confidence: 0.82,
            reason: "Based on your deep-dive patterns".to_string(),
        };

        assert_eq!(prediction.action, "enrich");
        assert!(prediction.confidence > 0.7);
    }

    #[test]
    fn test_user_defaults_inference() {
        let defaults = UserDefaults {
            preferred_scope: Some("security".to_string()),
            preferred_concept_type: Some("TECHNIQUE".to_string()),
            preferred_enrichment_depth: "balanced".to_string(),
            preferred_suggestion_count: 5,
            auto_accept_confidence: 0.85,
            verbosity: "normal".to_string(),
        };

        assert_eq!(defaults.preferred_enrichment_depth, "balanced");
        assert_eq!(defaults.preferred_suggestion_count, 5);
    }
}
