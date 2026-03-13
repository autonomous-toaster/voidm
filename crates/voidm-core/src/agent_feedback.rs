//! Agent feedback processing: how agents improve the memory system
//!
//! Agents can provide feedback on concepts:
//! - "helpful": useful concept, keep it
//! - "duplicate": merge with another concept
//! - "missing": concept should exist but doesn't
//! - "contradictory": conflicting information detected
//! - "underspecified": concept description too vague

use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct AgentFeedback {
    pub agent_id: String,
    pub feedback_type: String,  // helpful, duplicate, missing, contradictory, underspecified
    pub concept_id: Option<String>,
    pub concept_name: String,
    pub message: String,
    pub timestamp: chrono::DateTime<Utc>,
    pub context: Option<String>,  // What was agent doing? "search", "enrich", "reasoning"
}

/// Record agent feedback
pub async fn record_feedback(
    pool: &SqlitePool,
    agent_id: &str,
    feedback_type: &str,
    concept_name: &str,
    message: &str,
    context: Option<&str>,
) -> Result<AgentFeedback> {
    // Find concept ID by name
    let concept_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM ontology_concepts WHERE lower(name) = lower(?) LIMIT 1"
    )
    .bind(concept_name)
    .fetch_optional(pool)
    .await?;

    // Store feedback
    sqlx::query(
        "INSERT INTO agent_feedback (agent_id, feedback_type, concept_id, concept_name, message, timestamp, context)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(agent_id)
    .bind(feedback_type)
    .bind(&concept_id)
    .bind(concept_name)
    .bind(message)
    .bind(Utc::now())
    .bind(context)
    .execute(pool)
    .await?;

    Ok(AgentFeedback {
        agent_id: agent_id.to_string(),
        feedback_type: feedback_type.to_string(),
        concept_id,
        concept_name: concept_name.to_string(),
        message: message.to_string(),
        timestamp: Utc::now(),
        context: context.map(|s| s.to_string()),
    })
}

/// Analyze feedback patterns
pub async fn analyze_feedback_patterns(pool: &SqlitePool) -> Result<FeedbackAnalysis> {
    // Count feedback by type
    let feedback_counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT feedback_type, COUNT(*) as count
         FROM agent_feedback
         GROUP BY feedback_type
         ORDER BY count DESC"
    )
    .fetch_all(pool)
    .await?;

    // Most problematic concepts (most negative feedback)
    let problematic: Vec<(String, i64)> = sqlx::query_as(
        "SELECT concept_name, COUNT(*) as count
         FROM agent_feedback
         WHERE feedback_type IN ('duplicate', 'contradictory', 'underspecified')
         GROUP BY concept_name
         ORDER BY count DESC
         LIMIT 10"
    )
    .fetch_all(pool)
    .await?;

    // Most helpful concepts (most positive feedback)
    let helpful: Vec<(String, i64)> = sqlx::query_as(
        "SELECT concept_name, COUNT(*) as count
         FROM agent_feedback
         WHERE feedback_type = 'helpful'
         GROUP BY concept_name
         ORDER BY count DESC
         LIMIT 10"
    )
    .fetch_all(pool)
    .await?;

    Ok(FeedbackAnalysis {
        total_feedback: feedback_counts.iter().map(|(_, c)| c).sum(),
        by_type: feedback_counts,
        problematic_concepts: problematic,
        helpful_concepts: helpful,
    })
}

#[derive(Debug, Clone)]
pub struct FeedbackAnalysis {
    pub total_feedback: i64,
    pub by_type: Vec<(String, i64)>,
    pub problematic_concepts: Vec<(String, i64)>,
    pub helpful_concepts: Vec<(String, i64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_feedback_creation() {
        let feedback = AgentFeedback {
            agent_id: "pi".to_string(),
            feedback_type: "helpful".to_string(),
            concept_id: Some("123".to_string()),
            concept_name: "JWT".to_string(),
            message: "This concept is very useful for auth discussions".to_string(),
            timestamp: Utc::now(),
            context: Some("reasoning:security".to_string()),
        };

        assert_eq!(feedback.agent_id, "pi");
        assert_eq!(feedback.feedback_type, "helpful");
        assert_eq!(feedback.concept_name, "JWT");
    }

    #[test]
    fn test_duplicate_feedback() {
        let feedback = AgentFeedback {
            agent_id: "claude".to_string(),
            feedback_type: "duplicate".to_string(),
            concept_id: Some("456".to_string()),
            concept_name: "Dockerfile".to_string(),
            message: "This is basically the same as Docker, should merge".to_string(),
            timestamp: Utc::now(),
            context: Some("enrich:containers".to_string()),
        };

        assert_eq!(feedback.feedback_type, "duplicate");
        assert!(feedback.message.contains("merge"));
    }

    #[test]
    fn test_missing_concept_feedback() {
        let feedback = AgentFeedback {
            agent_id: "pi".to_string(),
            feedback_type: "missing".to_string(),
            concept_id: None,
            concept_name: "async-patterns".to_string(),
            message: "I need a concept for async/await patterns in Rust".to_string(),
            timestamp: Utc::now(),
            context: Some("search:async".to_string()),
        };

        assert_eq!(feedback.feedback_type, "missing");
        assert!(feedback.concept_id.is_none());
    }

    #[test]
    fn test_contradictory_feedback() {
        let feedback = AgentFeedback {
            agent_id: "gpt".to_string(),
            feedback_type: "contradictory".to_string(),
            concept_id: Some("789".to_string()),
            concept_name: "REST".to_string(),
            message: "Found contradictory info: some docs say REST requires HTTP, others don't".to_string(),
            timestamp: Utc::now(),
            context: Some("reasoning:api".to_string()),
        };

        assert_eq!(feedback.feedback_type, "contradictory");
    }
}
