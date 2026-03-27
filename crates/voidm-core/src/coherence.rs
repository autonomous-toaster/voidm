/// Coherence scoring for memory chunks.
///
/// Scores measure semantic completeness and readability of chunks.
/// Uses 5 components weighted together:
/// - Completeness (0.3): Does chunk cover a complete thought?
/// - Coherence (0.2): Are sentences logically connected?
/// - Relevance (0.2): Do all sentences relate to main topic?
/// - Specificity (0.15): Are claims specific vs vague?
/// - Metadata (0.15): Are proper nouns / references included?

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct CoherenceScore {
    /// Completeness: 0.0-1.0 (0.3 weight)
    pub completeness: f32,
    /// Coherence: 0.0-1.0 (0.2 weight)
    pub coherence: f32,
    /// Relevance: 0.0-1.0 (0.2 weight)
    pub relevance: f32,
    /// Specificity: 0.0-1.0 (0.15 weight)
    pub specificity: f32,
    /// Metadata: 0.0-1.0 (0.15 weight)
    pub metadata: f32,
}

impl CoherenceScore {
    /// Compute weighted average (final score 0.0-1.0).
    pub fn final_score(&self) -> f32 {
        self.completeness * 0.3
            + self.coherence * 0.2
            + self.relevance * 0.2
            + self.specificity * 0.15
            + self.metadata * 0.15
    }

    /// Human-readable quality level.
    pub fn quality_level(&self) -> &'static str {
        let score = self.final_score();
        if score >= 0.8 {
            "🟣 EXCELLENT"
        } else if score >= 0.6 {
            "🟢 GOOD"
        } else if score >= 0.3 {
            "🟡 FAIR"
        } else {
            "🔴 POOR"
        }
    }

    /// Format as pipe-separated scores for logging.
    pub fn format_log(&self) -> String {
        format!(
            "{:.2}|{:.2}|{:.2}|{:.2}|{:.2} → {:.2}",
            self.completeness,
            self.coherence,
            self.relevance,
            self.specificity,
            self.metadata,
            self.final_score()
        )
    }
}

/// Estimate coherence of a chunk based on heuristics.
///
/// Heuristics:
/// - Sentence count: More sentences = more complete
/// - Connector words: and, but, however, therefore, because, also, additionally
/// - Sentence transitions: Penalize abrupt topic shifts
/// - Keyword overlap: Repeated terms indicate topical focus
/// - Length penalty: Very short chunks = lower coherence potential
/// - Capitalization: Multiple proper nouns = metadata score
///
/// # Example
/// ```
/// let score = estimate_coherence("First sentence. Second sentence.");
/// assert!(score.final_score() > 0.5);
/// ```
pub fn estimate_coherence(content: &str) -> CoherenceScore {
    // Split into sentences (better handling of abbreviations)
    let sentences: Vec<&str> = content
        .split(|c| c == '.' || c == '!' || c == '?')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    
    let sentence_count = sentences.len();
    let word_count = content.split_whitespace().count();

    // COMPLETENESS: More sentences = more complete thought
    // Scale: 1 sentence = 0.2, 2-3 = 0.5, 4+ = 0.9
    let completeness = match sentence_count {
        0 => 0.0,
        1 => 0.3,
        2 => 0.5,
        3 => 0.65,
        4 => 0.8,
        5..=10 => 0.95,
        _ => 1.0,
    };

    // COHERENCE: Presence of connector words + sentence transitions
    let connector_words = vec![
        " and ", " but ", " however ", " therefore ", " because ",
        " also ", " additionally ", " furthermore ", " moreover ",
        " likewise ", " conversely ", " instead ", " meanwhile ",
    ];
    let connector_count = connector_words
        .iter()
        .filter(|conn| content.contains(**conn))
        .count();
    
    // Check for abrupt transitions (sentences with minimal word overlap)
    let mut topic_shift_count = 0;
    for i in 0..sentences.len().saturating_sub(1) {
        let curr_words: Vec<_> = sentences[i]
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();
        let next_words: Vec<_> = sentences[i + 1]
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();
        
        let overlap = curr_words.iter()
            .filter(|w| next_words.contains(w))
            .count();
        
        if overlap < 2 {
            topic_shift_count += 1; // Abrupt transition
        }
    }
    
    // Coherence score: connectors + topic continuity
    let coherence_base = if connector_count > 0 { 0.8 } else { 0.5 };
    let topic_penalty = (topic_shift_count as f32) * 0.05; // -5% per abrupt shift
    let coherence = (coherence_base - topic_penalty).max(0.2).min(1.0);

    // RELEVANCE: Penalize if too many unique concepts (topic jumping)
    let unique_words = content
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<std::collections::HashSet<_>>()
        .len();
    
    let word_uniqueness_ratio = unique_words as f32 / word_count.max(1) as f32;
    // If >70% unique words, likely topic jumping
    let relevance = if word_uniqueness_ratio > 0.7 {
        0.4 // Low relevance: too many new concepts
    } else if word_uniqueness_ratio > 0.5 {
        0.6 // Medium: some repetition
    } else {
        0.85 // Good: focused topic
    };

    // SPECIFICITY: Numbers, dates, specific terms vs vague language
    let has_numbers = content.chars().any(|c| c.is_numeric());
    let vague_words = vec!["very", "quite", "some", "maybe", "probably", "possibly"];
    let vague_count = vague_words
        .iter()
        .filter(|vague| content.contains(**vague))
        .count();
    
    let specificity_base = if has_numbers { 0.8 } else { 0.6 };
    let vague_penalty = (vague_count as f32) * 0.1; // -10% per vague word
    let specificity = (specificity_base - vague_penalty).max(0.2).min(1.0);

    // METADATA: Proper nouns (capitalized words, uppercase acronyms)
    let capitalized_words = content
        .split_whitespace()
        .filter(|w| {
            w.chars()
                .next()
                .map_or(false, |c| c.is_uppercase())
        })
        .count();
    
    let all_caps_acronyms = content
        .split_whitespace()
        .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_uppercase()))
        .count();
    
    let metadata = ((capitalized_words as f32 + all_caps_acronyms as f32 * 2.0) / 10.0)
        .min(1.0)
        .max(0.1);

    CoherenceScore {
        completeness,
        coherence,
        relevance,
        specificity,
        metadata,
    }
}

/// Estimate coherence with more aggressive heuristics (for comparison).
pub fn estimate_coherence_verbose(content: &str) -> (CoherenceScore, String) {
    let sentences: Vec<&str> = content.split_terminator('.').collect();
    let sentence_count = sentences.len();
    let word_count = content.split_whitespace().count();

    let completeness = ((sentence_count as f32).min(5.0) / 5.0).min(1.0);

    let has_connectors = content.contains(" and ")
        || content.contains(" but ")
        || content.contains(" however ")
        || content.contains(" therefore ")
        || content.contains(" because ");
    let coherence = if has_connectors { 0.75 } else { 0.5 };

    let relevance = if sentence_count <= 1 {
        0.5
    } else {
        0.7
    };

    let has_specificity = content.chars().any(|c| c.is_numeric())
        || content.chars().filter(|c| c.is_uppercase()).count() > 3;
    let specificity = if has_specificity { 0.75 } else { 0.5 };

    let capitalized_words = content
        .split_whitespace()
        .filter(|w| w.chars().next().map_or(false, |c| c.is_uppercase()))
        .count();
    let metadata = ((capitalized_words as f32) / 5.0).min(1.0);

    let debug_info = format!(
        "sentences={}, words={}, connectors={}, specificity={}, proper_nouns={}",
        sentence_count, word_count, has_connectors, has_specificity, capitalized_words
    );

    (
        CoherenceScore {
            completeness,
            coherence,
            relevance,
            specificity,
            metadata,
        },
        debug_info,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coherence_simple_sentence() {
        let score = estimate_coherence("Hello world.");
        assert!(score.final_score() > 0.0);
        assert!(score.final_score() < 1.0);
    }

    #[test]
    fn test_coherence_with_connectors() {
        let score = estimate_coherence("First sentence. And second. But also third.");
        assert!(score.coherence > 0.5);
    }

    #[test]
    fn test_coherence_with_numbers() {
        let score = estimate_coherence("OAuth2 is protocol. It uses 2048-bit keys.");
        assert!(score.specificity > 0.5);
    }

    #[test]
    fn test_coherence_format_log() {
        let score = CoherenceScore {
            completeness: 0.8,
            coherence: 0.75,
            relevance: 0.85,
            specificity: 0.70,
            metadata: 0.90,
        };
        let log = score.format_log();
        assert!(log.contains("0.80"));
        assert!(log.contains("0.75"));
        assert!(log.contains("→"));
    }

    #[test]
    fn test_quality_level_excellent() {
        let score = CoherenceScore {
            completeness: 0.9,
            coherence: 0.9,
            relevance: 0.9,
            specificity: 0.9,
            metadata: 0.9,
        };
        assert!(score.quality_level().contains("EXCELLENT"));
    }

    #[test]
    fn test_quality_level_good() {
        let score = CoherenceScore {
            completeness: 0.7,
            coherence: 0.7,
            relevance: 0.7,
            specificity: 0.7,
            metadata: 0.7,
        };
        assert!(score.quality_level().contains("GOOD"));
    }

    #[test]
    fn test_quality_level_fair() {
        let score = CoherenceScore {
            completeness: 0.5,
            coherence: 0.5,
            relevance: 0.5,
            specificity: 0.5,
            metadata: 0.5,
        };
        assert!(score.quality_level().contains("FAIR"));
    }

    #[test]
    fn test_quality_level_poor() {
        let score = CoherenceScore {
            completeness: 0.1,
            coherence: 0.1,
            relevance: 0.1,
            specificity: 0.1,
            metadata: 0.1,
        };
        assert!(score.quality_level().contains("POOR"));
    }

    #[test]
    fn test_coherence_verbose() {
        let content = "OAuth2 is a protocol. It uses 2048-bit keys. Therefore, it's secure.";
        let (score, debug) = estimate_coherence_verbose(content);
        assert!(!debug.is_empty());
        assert!(debug.contains("sentences"));
        assert!(score.final_score() > 0.0);
    }

    #[test]
    fn test_final_score_in_range() {
        let score = estimate_coherence("Any content here.");
        let final_score = score.final_score();
        assert!(final_score >= 0.0 && final_score <= 1.0);
    }
}
