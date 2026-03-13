//! Semantic deduplication: detect duplicates and relationships
//!
//! Instead of merging everything, use multi-signal scoring to:
//! - Merge: High similarity, same type, same scope (true duplicates)
//! - IS_A: One is more specific than the other (hierarchy)
//! - RELATES_TO: Related but distinct

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct DeduplicationSignal {
    /// Name similarity (Jaro-Winkler 0.0-1.0)
    pub name_similarity: f32,
    /// Type match (1.0 if same type, 0.0 otherwise)
    pub type_match: f32,
    /// Scope match (1.0 same, 0.7 same prefix, 0.0 different)
    pub scope_match: f32,
    /// Description overlap (embedding similarity if both have descriptions)
    pub description_overlap: f32,
    /// Final dedup score (0.0-1.0)
    pub dedup_score: f32,
    /// Recommended action: "merge", "is_a", "relates_to", None
    pub recommended_action: Option<String>,
}

impl DeduplicationSignal {
    /// Create signal from two concepts
    pub fn from_concepts(
        name1: &str,
        type1: Option<&str>,
        scope1: Option<&str>,
        name2: &str,
        type2: Option<&str>,
        scope2: Option<&str>,
    ) -> Self {
        // 1. Name similarity
        let name_similarity = strsim::jaro_winkler(&name1.to_lowercase(), &name2.to_lowercase()) as f32;

        // 2. Type match
        let type_match = if type1 == type2 && type1.is_some() { 1.0 } else { 0.0 };

        // 3. Scope match
        let scope_match = match (scope1, scope2) {
            (Some(s1), Some(s2)) if s1 == s2 => 1.0,
            (Some(s1), Some(s2)) => {
                // Same prefix (project/auth vs project/acme) = partial match
                let prefix1 = s1.split('/').next().unwrap_or("");
                let prefix2 = s2.split('/').next().unwrap_or("");
                if prefix1 == prefix2 { 0.7 } else { 0.0 }
            }
            (None, None) => 0.9,  // Both unscoped = high match
            _ => 0.0,              // One scoped, one not = no match
        };

        // 4. Description overlap (simplified: would use embeddings in reality)
        // For now, check if one name contains other (semantic signal)
        let description_overlap = if name1.to_lowercase().contains(&name2.to_lowercase())
            || name2.to_lowercase().contains(&name1.to_lowercase())
        {
            0.8
        } else {
            0.0
        };

        // Weighted score
        let dedup_score = name_similarity * 0.4 + type_match * 0.3 + scope_match * 0.2 + description_overlap * 0.1;

        // Recommend action
        let recommended_action = if dedup_score >= 0.90 {
            Some("merge".to_string())  // True duplicate
        } else if dedup_score >= 0.70 && name_similarity > 0.80 {
            Some("is_a".to_string())  // Hierarchical relationship
        } else if dedup_score >= 0.50 {
            Some("relates_to".to_string())  // Related concepts
        } else {
            None  // Keep separate
        };

        DeduplicationSignal {
            name_similarity,
            type_match,
            scope_match,
            description_overlap,
            dedup_score,
            recommended_action,
        }
    }
}

/// Find potential duplicates or related concepts
pub async fn find_dedup_candidates(
    pool: &SqlitePool,
    concept_name: &str,
    concept_type: Option<&str>,
    concept_scope: Option<&str>,
    threshold: f32,
) -> Result<Vec<(String, String, DeduplicationSignal)>> {
    // Get all concepts
    let concepts: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, name, concept_type, scope FROM ontology_concepts WHERE lower(name) != lower(?) ORDER BY name"
    )
    .bind(concept_name)
    .fetch_all(pool)
    .await?;

    let mut candidates = Vec::new();

    for (id, other_name, other_type, other_scope) in concepts {
        let signal = DeduplicationSignal::from_concepts(
            concept_name,
            concept_type,
            concept_scope,
            &other_name,
            other_type.as_deref(),
            other_scope.as_deref(),
        );

        if signal.dedup_score >= threshold {
            candidates.push((id, other_name, signal));
        }
    }

    // Sort by score descending
    candidates.sort_by(|a, b| b.2.dedup_score.partial_cmp(&a.2.dedup_score).unwrap());

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_signal_exact_match() {
        let signal = DeduplicationSignal::from_concepts(
            "JWT",
            Some("TECHNIQUE"),
            Some("projects/auth"),
            "JWT",
            Some("TECHNIQUE"),
            Some("projects/auth"),
        );

        assert!(signal.dedup_score >= 0.95);
        assert_eq!(signal.recommended_action, Some("merge".to_string()));
    }

    #[test]
    fn test_dedup_signal_hierarchy() {
        let signal = DeduplicationSignal::from_concepts(
            "JWT",
            Some("TECHNIQUE"),
            Some("projects/auth"),
            "JSON Web Token",
            Some("TECHNIQUE"),
            Some("projects/auth"),
        );

        // Should suggest relationship (names may not be that similar though)
        assert!(signal.dedup_score >= 0.50);
    }

    #[test]
    fn test_dedup_signal_similar_names() {
        let signal = DeduplicationSignal::from_concepts(
            "microservice",
            Some("ARCHITECTURE"),
            Some("projects/core"),
            "microservices",
            Some("ARCHITECTURE"),
            Some("projects/core"),
        );

        // Very similar names, same type, same scope = should suggest merge or is_a
        assert!(signal.name_similarity > 0.95);
        assert!(signal.dedup_score >= 0.80);
    }

    #[test]
    fn test_dedup_signal_different_scope() {
        let signal = DeduplicationSignal::from_concepts(
            "JWT",
            Some("TECHNIQUE"),
            Some("projects/auth"),
            "JWT",
            Some("TECHNIQUE"),
            Some("projects/payments"),
        );

        // Same name and type, but different scope = should suggest relationship
        assert!(signal.name_similarity > 0.95);  // Names are very similar
        // Score = 0.95*0.4 + 1.0*0.3 + 0.7*0.2 + 0.0*0.1 = 0.38 + 0.3 + 0.14 = 0.82
        assert!(signal.dedup_score > 0.70);
    }

    #[test]
    fn test_dedup_signal_different_type() {
        let signal = DeduplicationSignal::from_concepts(
            "testing",
            Some("TECHNIQUE"),
            Some("projects/core"),
            "testing",
            Some("METHODOLOGY"),
            Some("projects/core"),
        );

        // Same name and scope, but different type
        assert!(signal.name_similarity > 0.95);
        assert_eq!(signal.type_match, 0.0);  // No type match
        // Score = 0.95*0.4 + 0.0*0.3 + 0.9*0.2 + 0.0*0.1 = 0.38 + 0.18 = 0.56
        assert!(signal.dedup_score > 0.50);
    }

    #[test]
    fn test_dedup_signal_distinct() {
        let signal = DeduplicationSignal::from_concepts(
            "JWT",
            Some("TECHNIQUE"),
            Some("projects/auth"),
            "REST",
            Some("ARCHITECTURE"),
            Some("projects/api"),
        );

        // Quite different (not 0 similarity but low)
        assert!(signal.name_similarity < 0.70);
        assert_eq!(signal.type_match, 0.0);
        assert!(signal.dedup_score < 0.50);
        assert_eq!(signal.recommended_action, None);
    }
}
