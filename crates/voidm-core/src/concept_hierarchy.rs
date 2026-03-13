//! Parent concept detection: build IS_A hierarchies
//!
//! For each concept, suggest parent concepts that it IS_A child of.
//! Uses name analysis, description matching, and semantic signals.

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct ParentSuggestion {
    /// Potential parent concept ID
    pub parent_id: String,
    /// Parent concept name
    pub parent_name: String,
    /// How confident is the IS_A relationship
    pub confidence: f32,
    /// Why: "name_contains", "description_mention", "semantic", etc
    pub signal_type: String,
}

/// Suggest parent concepts for a given concept
pub async fn suggest_parents(
    pool: &SqlitePool,
    concept_name: &str,
    concept_description: Option<&str>,
) -> Result<Vec<ParentSuggestion>> {
    // Get all concepts
    let concepts: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, description FROM ontology_concepts WHERE lower(name) != lower(?) ORDER BY name"
    )
    .bind(concept_name)
    .fetch_all(pool)
    .await?;

    let mut suggestions = Vec::new();

    for (parent_id, parent_name, parent_desc) in concepts {
        // Signal 1: Name hierarchy (specific word is in broader concept)
        // e.g., "JWT" -> "authentication", "microservice" -> "architecture"
        if let Some(signal) = name_hierarchy_signal(concept_name, &parent_name) {
            suggestions.push(ParentSuggestion {
                parent_id: parent_id.clone(),
                parent_name: parent_name.clone(),
                confidence: signal,
                signal_type: "name_hierarchy".to_string(),
            });
            continue;
        }

        // Signal 2: Description mentions child
        // e.g., parent description says "includes JWT, OAuth, etc"
        if let Some(parent_desc_text) = &parent_desc {
            if parent_desc_text.to_lowercase().contains(&concept_name.to_lowercase()) {
                suggestions.push(ParentSuggestion {
                    parent_id: parent_id.clone(),
                    parent_name: parent_name.clone(),
                    confidence: 0.85,
                    signal_type: "description_mention".to_string(),
                });
                continue;
            }
        }

        // Signal 3: Concept description mentions parent
        // e.g., concept description says "a type of authentication"
        if let Some(concept_desc_text) = concept_description {
            if concept_desc_text.to_lowercase().contains(&parent_name.to_lowercase()) {
                suggestions.push(ParentSuggestion {
                    parent_id: parent_id.clone(),
                    parent_name: parent_name.clone(),
                    confidence: 0.80,
                    signal_type: "self_description".to_string(),
                });
            }
        }
    }

    // Sort by confidence descending
    suggestions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    Ok(suggestions)
}

/// Detect name-based hierarchy signals
/// Returns confidence if parent is likely parent of child
fn name_hierarchy_signal(child_name: &str, parent_name: &str) -> Option<f32> {
    let child_lower = child_name.to_lowercase();
    let parent_lower = parent_name.to_lowercase();

    // 1. Child name contains parent name as substring
    if child_lower.contains(&parent_lower) && parent_lower.len() > 2 {
        return Some(0.90);
    }

    // 2. Common parent-child patterns
    let child_patterns = [
        ("JWT", "authentication"),
        ("JWT", "security"),
        ("OAuth", "authentication"),
        ("OAuth", "security"),
        ("microservice", "architecture"),
        ("monolithic", "architecture"),
        ("REST", "API"),
        ("GraphQL", "API"),
        ("testing", "quality"),
        ("TDD", "methodology"),
        ("CI/CD", "methodology"),
        ("caching", "optimization"),
        ("indexing", "optimization"),
    ];

    for (specific, general) in &child_patterns {
        if child_lower.contains(&specific.to_lowercase()) && parent_lower.contains(&general.to_lowercase()) {
            return Some(0.85);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_hierarchy_substring() {
        let signal = name_hierarchy_signal("JWT token", "JWT");
        assert_eq!(signal, Some(0.90));

        let signal = name_hierarchy_signal("JWT", "JSON Web Token");
        assert_eq!(signal, None);  // Reverse direction
    }

    #[test]
    fn test_name_hierarchy_pattern() {
        let signal = name_hierarchy_signal("JWT", "authentication");
        assert_eq!(signal, Some(0.85));

        let signal = name_hierarchy_signal("REST", "API");
        assert_eq!(signal, Some(0.85));

        let signal = name_hierarchy_signal("microservice", "architecture");
        assert_eq!(signal, Some(0.85));
    }

    #[test]
    fn test_name_hierarchy_no_match() {
        let signal = name_hierarchy_signal("JWT", "REST");
        assert_eq!(signal, None);

        let signal = name_hierarchy_signal("testing", "foo");
        assert_eq!(signal, None);
    }

    #[test]
    fn test_parent_suggestion_creation() {
        let suggestion = ParentSuggestion {
            parent_id: "123".to_string(),
            parent_name: "authentication".to_string(),
            confidence: 0.85,
            signal_type: "name_hierarchy".to_string(),
        };

        assert_eq!(suggestion.parent_name, "authentication");
        assert!(suggestion.confidence > 0.80);
    }
}
