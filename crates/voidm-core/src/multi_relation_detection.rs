//! Multi-relation edge detection using NLI
//!
//! Detect semantic relationships between memories and concepts:
//! - SUPPORTS: memory agrees with concept
//! - CONTRADICTS: memory disagrees with concept
//! - EXEMPLIFIES: memory is specific example of concept
//! - DERIVES_FROM: concept derived from principle in memory

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct RelationDetection {
    pub memory_id: String,
    pub concept_id: String,
    pub relation_type: String,
    pub confidence: f32,
}

/// Detect semantic relations between memory and concept
/// Uses NLI scores to determine relationship type
pub fn detect_relations_from_nli(
    memory_content: &str,
    concept_name: &str,
    concept_description: Option<&str>,
    nli_entailment: f32,
    nli_contradiction: f32,
    nli_neutral: f32,
    cosine_similarity: f32,
) -> Option<RelationDetection> {
    // Use NLI scores to determine relationship
    
    // Contradiction: clear disagreement
    if nli_contradiction > 0.75 {
        return Some(RelationDetection {
            memory_id: "".to_string(),  // Will be filled by caller
            concept_id: "".to_string(),
            relation_type: "CONTRADICTS".to_string(),
            confidence: nli_contradiction,
        });
    }

    // Support: entailment with moderate similarity
    if nli_entailment > 0.70 && cosine_similarity > 0.70 {
        return Some(RelationDetection {
            memory_id: "".to_string(),
            concept_id: "".to_string(),
            relation_type: "SUPPORTS".to_string(),
            confidence: nli_entailment * cosine_similarity,
        });
    }

    // Exemplify: neutral or entailment with high similarity
    if (nli_neutral > 0.60 || nli_entailment > 0.50) && cosine_similarity > 0.75 {
        return Some(RelationDetection {
            memory_id: "".to_string(),
            concept_id: "".to_string(),
            relation_type: "EXEMPLIFIES".to_string(),
            confidence: ((nli_neutral + nli_entailment) / 2.0) * cosine_similarity,
        });
    }

    // Relates_to: some connection but not strong
    if (nli_neutral > 0.65 || nli_entailment > 0.40) && cosine_similarity > 0.50 {
        return Some(RelationDetection {
            memory_id: "".to_string(),
            concept_id: "".to_string(),
            relation_type: "RELATES_TO".to_string(),
            confidence: ((nli_neutral * 0.6 + nli_entailment * 0.4) * cosine_similarity),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_contradicts() {
        let relation = detect_relations_from_nli(
            "Testing is unnecessary",
            "testing",
            Some("Testing improves code quality"),
            0.10,  // Low entailment
            0.85,  // High contradiction
            0.05,  // Low neutral
            0.80,  // Good similarity
        );

        assert!(relation.is_some());
        let rel = relation.unwrap();
        assert_eq!(rel.relation_type, "CONTRADICTS");
        assert!(rel.confidence > 0.75);
    }

    #[test]
    fn test_detect_supports() {
        let relation = detect_relations_from_nli(
            "Testing improves code quality",
            "testing",
            Some("Testing is good practice"),
            0.80,  // High entailment
            0.05,  // Low contradiction
            0.15,  // Low neutral
            0.75,  // Good similarity
        );

        assert!(relation.is_some());
        let rel = relation.unwrap();
        assert_eq!(rel.relation_type, "SUPPORTS");
    }

    #[test]
    fn test_detect_exemplifies() {
        let relation = detect_relations_from_nli(
            "We use JWT tokens for authentication",
            "authentication",
            Some("Auth can use many techniques"),
            0.60,  // Higher entailment
            0.05,  // Low contradiction
            0.35,  // Moderate neutral
            0.80,  // High similarity
        );

        assert!(relation.is_some());
        let rel = relation.unwrap();
        assert_eq!(rel.relation_type, "EXEMPLIFIES");
    }

    #[test]
    fn test_detect_relates_to() {
        let relation = detect_relations_from_nli(
            "Caching is used in optimization",
            "performance",
            Some("Performance considerations"),
            0.25,  // Low entailment
            0.05,  // Low contradiction
            0.70,  // High neutral
            0.60,  // Moderate similarity
        );

        assert!(relation.is_some());
        let rel = relation.unwrap();
        assert_eq!(rel.relation_type, "RELATES_TO");
    }

    #[test]
    fn test_detect_no_relation() {
        let relation = detect_relations_from_nli(
            "The weather is sunny",
            "JWT",
            Some("JWT authentication"),
            0.10,
            0.05,
            0.85,
            0.20,  // Very low similarity
        );

        // Should not detect relation with such low similarity
        assert!(relation.is_none());
    }
}
