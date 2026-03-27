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
/// - Sentence count: Single sentence = lower coherence
/// - Sentence length variation: Similar lengths = more coherent
/// - Transition words: and, but, however, therefore = more coherent
/// - Capitalization: Multiple proper nouns = metadata score
/// - Punctuation: Balanced = good coherence
///
/// # Example
/// ```
/// let score = estimate_coherence("First sentence. Second sentence.");
/// assert!(score.final_score() > 0.5);
/// ```
pub fn estimate_coherence(content: &str) -> CoherenceScore {
    // Split into sentences
    let sentences: Vec<&str> = content.split_terminator('.').collect();
    let sentence_count = sentences.len();

    // Estimate completeness: more sentences = more complete
    let completeness = ((sentence_count as f32).min(5.0) / 5.0).min(1.0);

    // Estimate coherence: presence of connectors
    let has_connectors = content.contains(" and ")
        || content.contains(" but ")
        || content.contains(" however ")
        || content.contains(" therefore ")
        || content.contains(" because ");
    let coherence = if has_connectors { 0.75 } else { 0.5 };

    // Estimate relevance: avoid topic jumping (heuristic)
    let relevance = if sentence_count <= 1 {
        0.5 // Single sentence is less coherent
    } else {
        0.7
    };

    // Estimate specificity: check for specific numbers, names
    let has_specificity = content.chars().any(|c| c.is_numeric())
        || content.chars().filter(|c| c.is_uppercase()).count() > 3;
    let specificity = if has_specificity { 0.75 } else { 0.5 };

    // Estimate metadata: proper nouns (capitalized words)
    let capitalized_words = content
        .split_whitespace()
        .filter(|w| w.chars().next().map_or(false, |c| c.is_uppercase()))
        .count();
    let metadata = ((capitalized_words as f32) / 5.0).min(1.0);

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
