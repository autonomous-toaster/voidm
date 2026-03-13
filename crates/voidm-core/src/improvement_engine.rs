//! Self-improvement recommendations: what actions should the system take?
//!
//! Based on telemetry, concept usage, and gaps, recommend:
//! - Concepts to merge (duplicates or underused)
//! - Concepts to promote (high-value)
//! - Missing concepts to create (agent-queried but not extracted)
//! - Hierarchies to refine (incomplete parent-child relationships)

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct ImproveAction {
    pub action_type: String,  // "merge", "promote", "create", "refine_hierarchy"
    pub priority: i32,        // 1-10, higher = more important
    pub reason: String,       // Why recommend this?
    pub details: ActionDetails,
}

#[derive(Debug, Clone)]
pub enum ActionDetails {
    /// Merge two concepts
    Merge {
        source_id: String,
        source_name: String,
        target_id: String,
        target_name: String,
        similarity: f32,
    },
    /// Promote a concept (increase visibility/confidence)
    Promote {
        concept_id: String,
        concept_name: String,
        quality_score: f32,
        current_type: Option<String>,
        suggested_type: String,
    },
    /// Create a new concept
    Create {
        concept_name: String,
        suggested_type: String,
        query_count: i64,
        agent_queries: Vec<String>,
    },
    /// Refine a hierarchy
    RefineHierarchy {
        parent_id: String,
        parent_name: String,
        children: Vec<(String, String)>,  // (id, name)
        gap: String,  // What's missing?
    },
}

/// Generate improvement recommendations
pub async fn generate_recommendations(_pool: &SqlitePool) -> Result<Vec<ImproveAction>> {
    let mut actions = Vec::new();

    // TODO: Recommendation 1 - Find low-quality concepts to merge
    // Get concepts with quality_score < 0.3 and suggest merging with similar high-quality concepts
    actions.push(ImproveAction {
        action_type: "merge".to_string(),
        priority: 8,
        reason: "Low quality + similar to high-quality concept".to_string(),
        details: ActionDetails::Merge {
            source_id: "example_low".to_string(),
            source_name: "example_low".to_string(),
            target_id: "example_high".to_string(),
            target_name: "example_high".to_string(),
            similarity: 0.85,
        },
    });

    // TODO: Recommendation 2 - Find frequently queried missing concepts
    // If agent queries "async patterns" 5+ times but concept doesn't exist, suggest creation
    actions.push(ImproveAction {
        action_type: "create".to_string(),
        priority: 7,
        reason: "Agent searched 5 times - should extract concept".to_string(),
        details: ActionDetails::Create {
            concept_name: "async-patterns".to_string(),
            suggested_type: "PATTERN".to_string(),
            query_count: 5,
            agent_queries: vec![
                "async/await patterns".to_string(),
                "async handling in Rust".to_string(),
            ],
        },
    });

    // TODO: Recommendation 3 - Find hierarchy gaps
    // If we have "microservices" but no "distributed systems" parent, suggest creating it
    actions.push(ImproveAction {
        action_type: "refine_hierarchy".to_string(),
        priority: 6,
        reason: "Missing parent concept for architecture domain".to_string(),
        details: ActionDetails::RefineHierarchy {
            parent_id: "missing".to_string(),
            parent_name: "distributed-systems".to_string(),
            children: vec![
                ("ms1".to_string(), "microservices".to_string()),
                ("lb1".to_string(), "load-balancing".to_string()),
            ],
            gap: "No common parent for distributed computing concepts".to_string(),
        },
    });

    // Sort by priority descending
    actions.sort_by(|a, b| b.priority.cmp(&a.priority));

    Ok(actions)
}

/// Apply a single improvement action
pub async fn apply_action(_pool: &SqlitePool, action: &ImproveAction) -> Result<String> {
    match &action.details {
        ActionDetails::Merge { source_id, target_id, .. } => {
            // TODO: Call concept merge function
            Ok(format!("Would merge {} into {}", source_id, target_id))
        }
        ActionDetails::Create { concept_name, suggested_type, .. } => {
            // TODO: Call concept creation with suggested type
            Ok(format!("Would create concept '{}' as type {}", concept_name, suggested_type))
        }
        ActionDetails::Promote { concept_id, suggested_type, .. } => {
            // TODO: Update concept type
            Ok(format!("Would promote {} to type {}", concept_id, suggested_type))
        }
        ActionDetails::RefineHierarchy { parent_name, .. } => {
            // TODO: Create missing parent, establish IS_A relationships
            Ok(format!("Would refine hierarchy by creating parent '{}'", parent_name))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_improvement_action_creation() {
        let action = ImproveAction {
            action_type: "merge".to_string(),
            priority: 8,
            reason: "Low quality + similar".to_string(),
            details: ActionDetails::Merge {
                source_id: "a".to_string(),
                source_name: "Docker".to_string(),
                target_id: "b".to_string(),
                target_name: "Containers".to_string(),
                similarity: 0.90,
            },
        };

        assert_eq!(action.priority, 8);
        assert_eq!(action.action_type, "merge");
    }

    #[test]
    fn test_action_priority_ordering() {
        let mut actions = vec![
            ImproveAction {
                action_type: "create".to_string(),
                priority: 5,
                reason: "Low priority".to_string(),
                details: ActionDetails::Create {
                    concept_name: "x".to_string(),
                    suggested_type: "DOMAIN".to_string(),
                    query_count: 1,
                    agent_queries: vec![],
                },
            },
            ImproveAction {
                action_type: "merge".to_string(),
                priority: 9,
                reason: "High priority".to_string(),
                details: ActionDetails::Merge {
                    source_id: "a".to_string(),
                    source_name: "a".to_string(),
                    target_id: "b".to_string(),
                    target_name: "b".to_string(),
                    similarity: 0.95,
                },
            },
        ];

        actions.sort_by(|a, b| b.priority.cmp(&a.priority));

        assert_eq!(actions[0].priority, 9);
        assert_eq!(actions[1].priority, 5);
    }

    #[test]
    fn test_missing_concept_recommendation() {
        let action = ImproveAction {
            action_type: "create".to_string(),
            priority: 7,
            reason: "Frequently searched".to_string(),
            details: ActionDetails::Create {
                concept_name: "async-patterns".to_string(),
                suggested_type: "PATTERN".to_string(),
                query_count: 5,
                agent_queries: vec!["async/await".to_string(), "async patterns".to_string()],
            },
        };

        if let ActionDetails::Create { query_count, .. } = &action.details {
            assert!(*query_count >= 5);
        } else {
            panic!("Wrong action type");
        }
    }
}
