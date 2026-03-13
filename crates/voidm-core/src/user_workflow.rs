//! User workflow pattern analysis: detect how users work and what they prefer
//!
//! Patterns: search→view→enrich, batch operations, refinement loops
//! Preferences: quick-access vs deep-dive, collaborative vs solo

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct WorkflowPattern {
    pub user_id: String,
    pub pattern_name: String,           // "quick-search", "deep-dive", "batch", "collaborative"
    pub sequence: Vec<String>,          // [search, view, enrich, feedback, ...]
    pub frequency: i64,                 // How often this pattern occurs
    pub success_rate: f32,              // % of times this pattern succeeds
    pub avg_cycle_time_ms: i64,        // Average duration from start to end
    pub confidence: f32,                // How confident are we about this pattern (0-1)
}

#[derive(Debug, Clone)]
pub struct UserWorkStyle {
    pub user_id: String,
    pub primary_pattern: WorkflowPattern,       // Most common pattern
    pub secondary_patterns: Vec<WorkflowPattern>, // Other frequent patterns
    pub is_explorer: bool,              // Follows many edges?
    pub is_creator: bool,               // Creates many memories?
    pub is_collaborator: bool,          // Gives feedback, merges?
    pub is_focused: bool,               // Stays in same domain?
    pub work_rhythm: String,            // "steady", "bursty", "async"
    pub preferred_pace: String,         // "quick" (< 500ms), "normal" (500-2000ms), "deep" (> 2000ms)
}

#[derive(Debug, Clone)]
pub struct SessionPattern {
    pub user_id: String,
    pub session_start: chrono::DateTime<chrono::Utc>,
    pub session_end: chrono::DateTime<chrono::Utc>,
    pub interaction_count: i64,
    pub interactions: Vec<(String, String)>,  // (type, target_name)
    pub primary_focus: String,          // Which domain/concept?
    pub tasks_completed: i64,
    pub tasks_abandoned: i64,
}

/// Detect workflow patterns in user interactions
pub async fn detect_workflow_patterns(pool: &SqlitePool, user_id: &str) -> Result<Vec<WorkflowPattern>> {
    // Get all interactions for this user
    let interactions: Vec<(String, String)> = sqlx::query_as(
        "SELECT interaction_type, result FROM user_interactions WHERE user_id = ? ORDER BY timestamp"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    if interactions.len() < 2 {
        return Ok(vec![]);
    }

    // Define pattern templates to match against
    let patterns_to_detect = vec![
        ("quick-search", vec!["search", "view"]),
        ("deep-dive", vec!["search", "view", "view", "enrich"]),
        ("enrichment", vec!["view", "enrich", "feedback"]),
        ("collaborative", vec!["search", "feedback", "merge"]),
        ("batch", vec!["create", "create", "enrich", "enrich"]),
    ];

    let mut detected = Vec::new();

    for (pattern_name, expected_sequence) in patterns_to_detect {
        let seq_len = expected_sequence.len();
        let mut matches = 0;
        let mut success_count = 0;

        // Slide a window across interactions looking for pattern matches
        for window in interactions.windows(seq_len) {
            let window_types: Vec<&str> = window.iter().map(|(t, _)| t.as_str()).collect();
            
            if window_types == expected_sequence {
                matches += 1;
                
                // Check if all in window succeeded
                let all_success = window.iter().all(|(_, r)| r == "success");
                if all_success {
                    success_count += 1;
                }
            }
        }

        if matches > 0 {
            let success_rate = success_count as f32 / matches as f32;
            
            // Estimate cycle time (rough estimate: 500ms per interaction in pattern)
            let avg_ms = (seq_len as i64) * 500;

            let confidence = (matches as f32 / interactions.len() as f32).min(1.0);

            let pattern = WorkflowPattern {
                user_id: user_id.to_string(),
                pattern_name: pattern_name.to_string(),
                sequence: expected_sequence.iter().map(|s| s.to_string()).collect(),
                frequency: matches,
                success_rate,
                avg_cycle_time_ms: avg_ms,
                confidence,
            };

            detected.push(pattern);
        }
    }

    // Sort by frequency (descending)
    detected.sort_by(|a, b| b.frequency.cmp(&a.frequency));

    Ok(detected)
}

/// Infer user work style from patterns
pub async fn infer_work_style(pool: &SqlitePool, user_id: &str) -> Result<Option<UserWorkStyle>> {
    let patterns = detect_workflow_patterns(pool, user_id).await?;

    if patterns.is_empty() {
        return Ok(None);
    }

    // Determine user characteristics
    let is_explorer = patterns.iter()
        .any(|p| p.pattern_name.contains("deep-dive") && p.frequency > 5);

    let is_creator: bool = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_interactions WHERE user_id = ? AND interaction_type = 'create' LIMIT 1"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map(|count: i64| count > 0)
    .unwrap_or(false);

    let is_collaborator: bool = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_interactions WHERE user_id = ? AND interaction_type = 'feedback' LIMIT 1"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map(|count: i64| count > 0)
    .unwrap_or(false);

    // Determine focus (concentration in one domain vs multi-domain)
    let focus_variety: Option<f32> = sqlx::query_scalar(
        "SELECT 1.0 - (COUNT(DISTINCT context) / COUNT(*))
         FROM user_interactions
         WHERE user_id = ? AND context IS NOT NULL"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let is_focused = focus_variety.map(|v| v > 0.7).unwrap_or(false);

    // Determine work rhythm
    let work_rhythm = determine_work_rhythm(pool, user_id).await?;
    let preferred_pace = determine_preferred_pace(pool, user_id).await?;

    Ok(Some(UserWorkStyle {
        user_id: user_id.to_string(),
        primary_pattern: patterns[0].clone(),
        secondary_patterns: patterns.into_iter().skip(1).take(2).collect(),
        is_explorer,
        is_creator,
        is_collaborator,
        is_focused,
        work_rhythm,
        preferred_pace,
    }))
}

/// Analyze a user session (sequence of interactions within time window)
pub async fn analyze_session(
    pool: &SqlitePool,
    user_id: &str,
    session_start: chrono::DateTime<chrono::Utc>,
    session_end: chrono::DateTime<chrono::Utc>,
) -> Result<Option<SessionPattern>> {
    let interactions: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT interaction_type, target_name, result
         FROM user_interactions
         WHERE user_id = ? AND timestamp BETWEEN ? AND ?
         ORDER BY timestamp"
    )
    .bind(user_id)
    .bind(session_start.to_rfc3339())
    .bind(session_end.to_rfc3339())
    .fetch_all(pool)
    .await?;

    if interactions.is_empty() {
        return Ok(None);
    }

    let interaction_count = interactions.len() as i64;
    let tasks_completed = interactions.iter()
        .filter(|(_, _, result)| result == "success")
        .count() as i64;
    let tasks_abandoned = interactions.iter()
        .filter(|(_, _, result)| result == "cancel")
        .count() as i64;

    // Primary focus is the most mentioned concept/domain
    let mut focus_count = std::collections::HashMap::new();
    for (_, target_name, _) in &interactions {
        *focus_count.entry(target_name.clone()).or_insert(0i64) += 1;
    }
    let primary_focus = focus_count.iter()
        .max_by_key(|(_, count)| *count)
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(Some(SessionPattern {
        user_id: user_id.to_string(),
        session_start,
        session_end,
        interaction_count,
        interactions: interactions.iter().map(|(t, n, _)| (t.clone(), n.clone())).collect(),
        primary_focus,
        tasks_completed,
        tasks_abandoned,
    }))
}

// Helper functions

async fn determine_work_rhythm(pool: &SqlitePool, user_id: &str) -> Result<String> {
    // Get inter-action time deltas (seconds between interactions)
    let interactions: Vec<(String,)> = sqlx::query_as(
        "SELECT timestamp FROM user_interactions WHERE user_id = ? ORDER BY timestamp"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    if interactions.len() < 2 {
        return Ok("unknown".to_string());
    }

    // Calculate time deltas between consecutive interactions
    let mut deltas = Vec::new();
    for i in 1..interactions.len() {
        if let (Ok(t1), Ok(t2)) = (
            chrono::DateTime::parse_from_rfc3339(&interactions[i-1].0),
            chrono::DateTime::parse_from_rfc3339(&interactions[i].0),
        ) {
            let delta_secs = (t2.timestamp() - t1.timestamp()).max(1);  // Minimum 1 second
            deltas.push(delta_secs);
        }
    }

    if deltas.is_empty() {
        return Ok("unknown".to_string());
    }

    // Calculate variance in deltas
    let mean = deltas.iter().sum::<i64>() as f32 / deltas.len() as f32;
    let variance = deltas.iter()
        .map(|d| {
            let diff = *d as f32 - mean;
            diff * diff
        })
        .sum::<f32>() / deltas.len() as f32;

    let std_dev = variance.sqrt();
    let cv = std_dev / mean;  // Coefficient of variation

    // Classify work rhythm:
    // - steady: low variance (cv < 0.5)
    // - bursty: medium variance (cv 0.5-2.0)
    // - async: high variance (cv > 2.0)
    let rhythm = match cv {
        cv if cv < 0.5 => "steady",
        cv if cv < 2.0 => "bursty",
        _ => "async",
    };

    Ok(rhythm.to_string())
}

async fn determine_preferred_pace(pool: &SqlitePool, user_id: &str) -> Result<String> {
    let avg_duration: Option<i64> = sqlx::query_scalar(
        "SELECT AVG(duration_ms) FROM user_interactions WHERE user_id = ?"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    match avg_duration {
        Some(ms) if ms < 500 => Ok("quick".to_string()),
        Some(ms) if ms < 2000 => Ok("normal".to_string()),
        Some(_) => Ok("deep".to_string()),
        None => Ok("unknown".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_workflow_pattern_creation() {
        let pattern = WorkflowPattern {
            user_id: "user-123".to_string(),
            pattern_name: "quick-search".to_string(),
            sequence: vec!["search".to_string(), "view".to_string()],
            frequency: 42,
            success_rate: 0.95,
            avg_cycle_time_ms: 300,
            confidence: 0.92,
        };

        assert_eq!(pattern.pattern_name, "quick-search");
        assert_eq!(pattern.frequency, 42);
        assert!(pattern.confidence > 0.9);
    }

    #[test]
    fn test_work_style_creation() {
        let primary = WorkflowPattern {
            user_id: "user-123".to_string(),
            pattern_name: "deep-dive".to_string(),
            sequence: vec![],
            frequency: 50,
            success_rate: 0.88,
            avg_cycle_time_ms: 2500,
            confidence: 0.91,
        };

        let work_style = UserWorkStyle {
            user_id: "user-123".to_string(),
            primary_pattern: primary,
            secondary_patterns: vec![],
            is_explorer: true,
            is_creator: true,
            is_collaborator: false,
            is_focused: true,
            work_rhythm: "steady".to_string(),
            preferred_pace: "deep".to_string(),
        };

        assert!(work_style.is_explorer);
        assert_eq!(work_style.preferred_pace, "deep");
    }

    #[test]
    fn test_session_pattern_creation() {
        let now = Utc::now();
        let session = SessionPattern {
            user_id: "user-123".to_string(),
            session_start: now - Duration::hours(1),
            session_end: now,
            interaction_count: 15,
            interactions: vec![
                ("search".to_string(), "JWT".to_string()),
                ("view".to_string(), "JWT".to_string()),
            ],
            primary_focus: "security".to_string(),
            tasks_completed: 12,
            tasks_abandoned: 1,
        };

        assert_eq!(session.interaction_count, 15);
        assert!(session.tasks_completed > session.tasks_abandoned);
    }
}
