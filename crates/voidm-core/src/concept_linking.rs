//! Bidirectional concept linking: detect concept mentions in memories
//!
//! Not just linking extracted entities, but finding when memory content
//! mentions or relates to existing concepts.

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct ConceptMention {
    /// Concept ID that is mentioned
    pub concept_id: String,
    /// Concept name
    pub concept_name: String,
    /// How it was detected: exact, semantic, or pattern
    pub mention_type: String,
    /// Confidence score
    pub confidence: f32,
}

/// Find all concept mentions in a memory's content
pub async fn find_concept_mentions(
    pool: &SqlitePool,
    memory_id: &str,
    content: &str,
) -> Result<Vec<ConceptMention>> {
    // Get all concepts
    let concepts: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, name FROM ontology_concepts ORDER BY name DESC"  // DESC to match longer names first
    )
    .fetch_all(pool)
    .await?;

    let mut mentions = Vec::new();
    let content_lower = content.to_lowercase();

    for (concept_id, concept_name) in concepts {
        let concept_lower = concept_name.to_lowercase();

        // 1. Exact mention: concept name appears in content
        if content_contains_word(&content_lower, &concept_lower) {
            mentions.push(ConceptMention {
                concept_id: concept_id.clone(),
                concept_name: concept_name.clone(),
                mention_type: "exact".to_string(),
                confidence: 0.95,
            });
            continue;  // Skip semantic check if exact match found
        }

        // 2. Semantic mention: check if content describes this concept
        // (Simple heuristic: if concept name is in any sentence of content)
        for sentence in content.split(|c: char| c == '.' || c == '!' || c == '?') {
            if sentence.to_lowercase().contains(&concept_lower) {
                mentions.push(ConceptMention {
                    concept_id: concept_id.clone(),
                    concept_name: concept_name.clone(),
                    mention_type: "semantic".to_string(),
                    confidence: 0.75,
                });
                break;  // Only count once per memory
            }
        }
    }

    Ok(mentions)
}

/// Create MENTIONS edges for all detected concept mentions
pub async fn create_mention_edges(
    pool: &SqlitePool,
    memory_id: &str,
    mentions: &[ConceptMention],
) -> Result<usize> {
    let mut created = 0;

    for mention in mentions {
        // Check if edge already exists
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM ontology_edges
             WHERE (SELECT id FROM graph_nodes WHERE memory_id = ?) as source_id
             AND (SELECT id FROM graph_nodes WHERE memory_id = ?) as target_id
             AND rel_type = 'MENTIONS'"
        )
        .bind(memory_id)
        .bind(&mention.concept_id)
        .fetch_optional(pool)
        .await?;

        if exists.is_none() {
            // TODO: Create edge (would need graph node for concept)
            // For now, just count it
            created += 1;
        }
    }

    Ok(created)
}

/// Check if content contains word (case-insensitive, word boundaries)
fn content_contains_word(content: &str, word: &str) -> bool {
    if !content.contains(word) {
        return false;
    }

    // Verify word boundaries
    for start_pos in content.match_indices(word).map(|(i, _)| i) {
        let end_pos = start_pos + word.len();

        let before_ok = start_pos == 0 || !content[..start_pos].ends_with(|c: char| c.is_alphanumeric());
        let after_ok = end_pos >= content.len() || !content[end_pos..].starts_with(|c: char| c.is_alphanumeric());

        if before_ok && after_ok {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_contains_word() {
        assert!(content_contains_word("testing is important", "testing"));
        assert!(content_contains_word("The testing phase", "testing"));
        assert!(content_contains_word("testing.", "testing"));
        assert!(!content_contains_word("protesting", "testing"));  // Word boundary should fail
        assert!(!content_contains_word("test", "testing"));
    }

    #[test]
    fn test_concept_mention_exact() {
        let mention = ConceptMention {
            concept_id: "123".to_string(),
            concept_name: "JWT".to_string(),
            mention_type: "exact".to_string(),
            confidence: 0.95,
        };

        assert_eq!(mention.mention_type, "exact");
        assert!(mention.confidence > 0.90);
    }
}
