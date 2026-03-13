//! Hybrid concept extraction: NER + keywords + patterns
//!
//! Combines multiple extraction strategies to identify domain concepts:
//! 1. Named entity recognition (high confidence, specific)
//! 2. Domain keyword matching (medium confidence, generic)
//! 3. Regex pattern matching (low confidence, specific patterns)
//!
//! Results aggregated with multi-source scoring.

use anyhow::Result;
use std::collections::HashMap;

// ─── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConceptSource {
    /// Named entity from NER (highest confidence)
    NerEntity,
    /// Domain keyword match (medium confidence)
    KeywordMatch,
    /// Regex pattern match (lower confidence, specific patterns)
    PatternMatch,
}

#[derive(Debug, Clone)]
pub struct SourceSignal {
    pub source: ConceptSource,
    pub confidence: f32,
}

/// Extracted concept with multiple signals and aggregated confidence
#[derive(Debug, Clone)]
pub struct ExtractedConcept {
    pub text: String,
    pub sources: Vec<SourceSignal>,
    /// Aggregate confidence (0.0-1.0)
    pub confidence: f32,
    /// Optional concept type for domain awareness
    pub concept_type: Option<String>,
}

/// Domain keyword definition
#[derive(Debug, Clone)]
pub struct DomainKeyword {
    pub text: &'static str,
    pub confidence: f32,
    pub concept_type: Option<&'static str>,
}

/// Regex pattern for concept matching
#[derive(Debug, Clone)]
pub struct ConceptPattern {
    pub pattern: &'static str,  // Regex pattern
    pub confidence: f32,
    pub concept_type: Option<&'static str>,
}

// ─── Domain Keywords (Configurable) ────────────────────────────────────────

/// Default domain keywords for software engineering domains
pub const DEFAULT_DOMAIN_KEYWORDS: &[DomainKeyword] = &[
    // Testing/verification
    DomainKeyword {
        text: "unit test",
        confidence: 0.95,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "integration test",
        confidence: 0.95,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "end-to-end test",
        confidence: 0.94,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "TDD",
        confidence: 0.92,
        concept_type: Some("METHODOLOGY"),
    },
    DomainKeyword {
        text: "mocking",
        confidence: 0.90,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "testing",
        confidence: 0.85,
        concept_type: Some("TECHNIQUE"),
    },

    // Architecture
    DomainKeyword {
        text: "microservice",
        confidence: 0.94,
        concept_type: Some("ARCHITECTURE"),
    },
    DomainKeyword {
        text: "monolithic",
        confidence: 0.92,
        concept_type: Some("ARCHITECTURE"),
    },
    DomainKeyword {
        text: "separation of concerns",
        confidence: 0.91,
        concept_type: Some("PRINCIPLE"),
    },
    DomainKeyword {
        text: "DRY",
        confidence: 0.90,
        concept_type: Some("PRINCIPLE"),
    },
    DomainKeyword {
        text: "SOLID",
        confidence: 0.92,
        concept_type: Some("PRINCIPLE"),
    },

    // Security
    DomainKeyword {
        text: "JWT",
        confidence: 0.96,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "RBAC",
        confidence: 0.95,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "encryption",
        confidence: 0.93,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "authentication",
        confidence: 0.92,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "authorization",
        confidence: 0.92,
        concept_type: Some("TECHNIQUE"),
    },

    // Performance
    DomainKeyword {
        text: "caching",
        confidence: 0.91,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "indexing",
        confidence: 0.90,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "optimization",
        confidence: 0.88,
        concept_type: Some("PROCESS"),
    },
    DomainKeyword {
        text: "latency",
        confidence: 0.87,
        concept_type: Some("DOMAIN"),
    },

    // Data
    DomainKeyword {
        text: "database",
        confidence: 0.92,
        concept_type: Some("DOMAIN"),
    },
    DomainKeyword {
        text: "schema",
        confidence: 0.90,
        concept_type: Some("DOMAIN"),
    },
    DomainKeyword {
        text: "migration",
        confidence: 0.85,
        concept_type: Some("PROCESS"),
    },
    DomainKeyword {
        text: "normalization",
        confidence: 0.88,
        concept_type: Some("TECHNIQUE"),
    },

    // DevOps/Infrastructure
    DomainKeyword {
        text: "containerization",
        confidence: 0.91,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "orchestration",
        confidence: 0.90,
        concept_type: Some("TECHNIQUE"),
    },
    DomainKeyword {
        text: "CI/CD",
        confidence: 0.93,
        concept_type: Some("METHODOLOGY"),
    },
    DomainKeyword {
        text: "deployment",
        confidence: 0.88,
        concept_type: Some("PROCESS"),
    },

    // Patterns
    DomainKeyword {
        text: "design pattern",
        confidence: 0.92,
        concept_type: Some("PATTERN"),
    },
    DomainKeyword {
        text: "observer pattern",
        confidence: 0.94,
        concept_type: Some("PATTERN"),
    },
    DomainKeyword {
        text: "singleton pattern",
        confidence: 0.94,
        concept_type: Some("PATTERN"),
    },
    DomainKeyword {
        text: "factory pattern",
        confidence: 0.94,
        concept_type: Some("PATTERN"),
    },
];

// ─── Concept Patterns (Pattern-based) ────────────────────────────────────────

/// Patterns for concept detection (simple string-based)
pub fn get_concept_patterns() -> Vec<(&'static str, f32, Option<&'static str>)> {
    vec![
        // "X pattern"
        ("pattern", 0.85, Some("PATTERN")),
        // "X testing" or "testing"
        ("testing", 0.80, Some("TECHNIQUE")),
        // "separation", "decoupling", "isolation"
        ("separation", 0.75, Some("PRINCIPLE")),
        ("decoupling", 0.75, Some("PRINCIPLE")),
        ("isolation", 0.75, Some("PRINCIPLE")),
        // CAP theorem concepts
        ("consistency", 0.70, Some("PRINCIPLE")),
        ("availability", 0.70, Some("PRINCIPLE")),
        ("partition tolerance", 0.70, Some("PRINCIPLE")),
    ]
}

// ─── Extraction ────────────────────────────────────────────────────────────

/// Extract concepts using hybrid approach: NER + keywords + patterns
pub async fn extract_concepts_hybrid(
    content: &str,
    _scope: Option<&str>,
) -> Result<Vec<ExtractedConcept>> {
    let mut concepts: HashMap<String, ExtractedConcept> = HashMap::new();

    // Layer 1: NER (high confidence, specific)
    let entities = crate::ner::extract_entities(content).unwrap_or_default();
    for entity in entities {
        let concept_type = entity_type_to_concept_type(&entity.entity_type);
        concepts.insert(
            entity.text.clone(),
            ExtractedConcept {
                text: entity.text,
                sources: vec![SourceSignal {
                    source: ConceptSource::NerEntity,
                    confidence: entity.score,
                }],
                confidence: entity.score,
                concept_type,
            },
        );
    }

    // Layer 2: Domain keywords (medium confidence, generic)
    for keyword in DEFAULT_DOMAIN_KEYWORDS {
        if content_contains_word(content, keyword.text) {
            let key = keyword.text.to_lowercase();
            concepts
                .entry(key)
                .and_modify(|c| {
                    c.sources.push(SourceSignal {
                        source: ConceptSource::KeywordMatch,
                        confidence: keyword.confidence,
                    });
                    update_aggregate_confidence(c);
                })
                .or_insert_with(|| ExtractedConcept {
                    text: keyword.text.to_string(),
                    sources: vec![SourceSignal {
                        source: ConceptSource::KeywordMatch,
                        confidence: keyword.confidence,
                    }],
                    confidence: keyword.confidence,
                    concept_type: keyword.concept_type.map(|s| s.to_string()),
                });
        }
    }

    // Layer 3: Patterns (lower confidence, specific matches)
    for (pattern_text, pattern_conf, pattern_type) in get_concept_patterns() {
        if content_contains_word(content, pattern_text) {
            let key = pattern_text.to_lowercase();

            concepts
                .entry(key)
                .and_modify(|c| {
                    c.sources.push(SourceSignal {
                        source: ConceptSource::PatternMatch,
                        confidence: pattern_conf,
                    });
                    update_aggregate_confidence(c);
                })
                .or_insert_with(|| ExtractedConcept {
                    text: pattern_text.to_string(),
                    sources: vec![SourceSignal {
                        source: ConceptSource::PatternMatch,
                        confidence: pattern_conf,
                    }],
                    confidence: pattern_conf,
                    concept_type: pattern_type.map(|s| s.to_string()),
                });
        }
    }

    // Sort by confidence descending
    let mut result: Vec<_> = concepts.into_values().collect();
    result.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    Ok(result)
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Check if content contains word (case-insensitive, word boundaries)
fn content_contains_word(content: &str, word: &str) -> bool {
    let content_lower = content.to_lowercase();
    let word_lower = word.to_lowercase();
    
    // Simple substring check with space/punctuation boundaries
    if !content_lower.contains(&word_lower) {
        return false;
    }
    
    // Verify word boundaries (not part of another word)
    for start_pos in content_lower.match_indices(&word_lower).map(|(i, _)| i) {
        let end_pos = start_pos + word_lower.len();
        
        let before_ok = start_pos == 0 || !content_lower[..start_pos].ends_with(|c: char| c.is_alphanumeric());
        let after_ok = end_pos >= content_lower.len() || !content_lower[end_pos..].starts_with(|c: char| c.is_alphanumeric());
        
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Update aggregate confidence from multiple sources
fn update_aggregate_confidence(concept: &mut ExtractedConcept) {
    if concept.sources.is_empty() {
        concept.confidence = 0.0;
        return;
    }

    // Average of top 2 sources weighted by source type
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;

    for signal in &concept.sources {
        let weight = match signal.source {
            ConceptSource::NerEntity => 0.6,     // Highest weight
            ConceptSource::KeywordMatch => 0.3,   // Medium weight
            ConceptSource::PatternMatch => 0.1,   // Lower weight
        };

        weighted_sum += signal.confidence * weight;
        weight_sum += weight;
    }

    concept.confidence = if weight_sum > 0.0 {
        weighted_sum / weight_sum
    } else {
        0.0
    };
}

/// Map NER entity type to concept type
fn entity_type_to_concept_type(entity_type: &str) -> Option<String> {
    match entity_type {
        "PER" => Some("ENTITY".to_string()),
        "ORG" => Some("ENTITY".to_string()),
        "LOC" => Some("DOMAIN".to_string()),
        "MISC" => None,  // Generic, no specific type
        _ => None,
    }
}

// ─── Filtering ────────────────────────────────────────────────────────────

/// Check if concept should be created
pub fn should_create_concept(
    text: &str,
    confidence: f32,
    concept_type: Option<&str>,
) -> bool {
    // 1. Length filter: 2+ words OR 4+ chars
    let word_count = text.split_whitespace().count();
    if word_count < 2 && text.len() < 4 {
        return false;
    }

    // 2. Confidence threshold varies by type
    let min_confidence = match concept_type {
        Some("TECHNIQUE") | Some("ARCHITECTURE") | Some("PATTERN") => 0.80,
        Some("METHODOLOGY") | Some("PRINCIPLE") => 0.75,
        Some("DOMAIN") | Some("PROCESS") => 0.70,
        Some("ENTITY") => 0.60,
        _ => 0.60,
    };

    if confidence < min_confidence {
        return false;
    }

    // 3. Noise filters
    if is_stopword(text) || is_temporal(text) || is_abbreviation_only(text) {
        return false;
    }

    true
}

/// Check if text is a stopword
fn is_stopword(text: &str) -> bool {
    let stopwords = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
        "of", "with", "by", "is", "are", "was", "were", "been", "be",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "may", "might", "must", "can", "that", "this", "these", "those",
    ];

    stopwords.contains(&text.to_lowercase().as_str())
}

/// Check if text is temporal (dates, times, durations)
fn is_temporal(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    
    // Year pattern (4 digits)
    if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    
    // Common temporal words
    let temporal_words = [
        "today", "tomorrow", "yesterday",
        "january", "february", "march", "april", "may", "june",
        "july", "august", "september", "october", "november", "december",
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
        "morning", "afternoon", "evening", "night",
    ];
    
    for word in &temporal_words {
        if text_lower.contains(word) {
            return true;
        }
    }
    
    false
}

/// Check if text is abbreviation-only (no actual content)
fn is_abbreviation_only(text: &str) -> bool {
    // Single capital letters or all caps with 1-2 chars
    text.len() <= 2 && text.chars().all(|c| c.is_ascii_uppercase() || !c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_create_concept() {
        // Valid concepts - adjust confidence to match thresholds
        assert!(should_create_concept("JWT token", 0.85, Some("TECHNIQUE")));
        assert!(should_create_concept("testing pattern", 0.80, Some("PATTERN")));
        assert!(should_create_concept("microservice", 0.80, Some("ARCHITECTURE")));  // threshold is 0.80

        // Too short
        assert!(!should_create_concept("a", 0.90, None));
        assert!(!should_create_concept("ab", 0.90, None));

        // Too low confidence (below thresholds)
        assert!(!should_create_concept("testing", 0.50, Some("TECHNIQUE")));
        assert!(!should_create_concept("testing", 0.55, None));  // None type requires 0.60, 0.55 is too low

        // Stopwords
        assert!(!should_create_concept("the", 0.90, None));
        assert!(!should_create_concept("and", 0.90, None));

        // Temporal
        assert!(!should_create_concept("2026", 0.90, None));
        assert!(!should_create_concept("today", 0.90, None));
    }

    #[test]
    fn test_entity_type_mapping() {
        assert_eq!(entity_type_to_concept_type("PER"), Some("ENTITY".to_string()));
        assert_eq!(entity_type_to_concept_type("ORG"), Some("ENTITY".to_string()));
        assert_eq!(entity_type_to_concept_type("LOC"), Some("DOMAIN".to_string()));
        assert_eq!(entity_type_to_concept_type("MISC"), None);
    }

    #[test]
    fn test_is_temporal() {
        assert!(is_temporal("2026"));
        assert!(is_temporal("today"));
        assert!(is_temporal("Monday"));
        assert!(is_temporal("January"));
        assert!(!is_temporal("testing"));
        assert!(!is_temporal("microservice"));
    }
}
