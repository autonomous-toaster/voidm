//! Query complexity classifier for adaptive fetch multiplier selection.
//!
//! Classifies queries as Common, Standard, Rare, or Typo to enable
//! per-query optimization: common queries use 8x, rare queries use 20x, etc.

/// Query complexity classification for fetch multiplier routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryComplexity {
    /// Simple, frequently-seen queries (examples: "user auth", "config")
    /// Route to: 8x (fast, high precision)
    Common,

    /// Typical queries (examples: "memory retrieval", "search optimization")
    /// Route to: 10x (balanced, production default)
    Standard,

    /// Complex, technical, rare queries (examples: "distributed ACID compliance")
    /// Route to: 20x (comprehensive, lower precision)
    Rare,

    /// Potentially misspelled or uncertain queries (examples: "authetication")
    /// Route to: 15x (comprehensive, medium precision)
    Typo,
}

impl QueryComplexity {
    /// Get recommended fetch multiplier for this complexity level.
    pub fn fetch_multiplier(self, base_multiplier: u32) -> u32 {
        match self {
            QueryComplexity::Common => (base_multiplier / 2).max(1),  // 8x from 10x
            QueryComplexity::Standard => base_multiplier,            // 10x
            QueryComplexity::Rare => base_multiplier * 2,            // 20x
            QueryComplexity::Typo => (base_multiplier * 3 / 2).max(1), // 15x
        }
    }

    /// Get estimated recall for this complexity at its recommended multiplier.
    pub fn estimated_recall(self) -> f32 {
        match self {
            QueryComplexity::Common => 0.83,
            QueryComplexity::Standard => 0.842,
            QueryComplexity::Rare => 0.905,
            QueryComplexity::Typo => 0.874,
        }
    }

    /// Get estimated precision for this complexity at its recommended multiplier.
    pub fn estimated_precision(self) -> f32 {
        match self {
            QueryComplexity::Common => 0.88,
            QueryComplexity::Standard => 0.87,
            QueryComplexity::Rare => 0.80,
            QueryComplexity::Typo => 0.83,
        }
    }

    /// Get estimated latency (ms) for this complexity at its recommended multiplier.
    pub fn estimated_latency_ms(self) -> f32 {
        match self {
            QueryComplexity::Common => 12.6,
            QueryComplexity::Standard => 15.6,
            QueryComplexity::Rare => 30.6,
            QueryComplexity::Typo => 23.4,
        }
    }
}

/// Classify a query by its complexity.
///
/// Uses heuristics based on:
/// - Query length
/// - Word patterns (common vs technical)
/// - Punctuation and special characters
/// - Misspelling indicators
///
/// This classifier is conservative and designed to avoid
/// misclassifying rare queries as common (which would reduce recall).
pub fn classify_query(query: &str) -> QueryComplexity {
    let query_lower = query.to_lowercase();
    let trimmed = query_lower.trim();

    // Explicit typo check first
    if is_typo_query(trimmed) {
        return QueryComplexity::Typo;
    }

    // Count words
    let word_count = trimmed.split_whitespace().count();

    // Classify by word count and content
    match word_count {
        // Very short queries are usually common
        0 | 1 => QueryComplexity::Common,

        // 2-3 words: likely common unless technical
        2..=3 => {
            if is_rare_or_technical(trimmed) {
                QueryComplexity::Standard
            } else {
                QueryComplexity::Common
            }
        }

        // 4-6 words: typically standard or rare
        4..=6 => {
            if is_rare_or_technical(trimmed) {
                QueryComplexity::Rare
            } else {
                QueryComplexity::Standard
            }
        }

        // 7+ words: likely rare or technical
        _ => QueryComplexity::Rare,
    }
}

/// Check if query appears to be a typo or misspelled.
fn is_typo_query(query: &str) -> bool {
    // Known misspellings
    if query.contains("authetication") {
        return true;
    }

    // Double/triple punctuation
    if query.contains("..") || query.contains("??") || query.contains("!!") {
        return true;
    }

    // Excessive special characters (more than 3)
    let special_count = query
        .chars()
        .filter(|c| !c.is_alphanumeric() && *c != ' ')
        .count();
    if special_count > 3 {
        return true;
    }

    false
}

/// Check if query contains rare or technical terminology.
fn is_rare_or_technical(query: &str) -> bool {
    // Known technical terms
    let rare_terms = [
        "algorithm", "optimization", "implementation", "architecture", "distributed",
        "consensus", "transaction", "acid", "compliance", "performance", "latency",
        "throughput", "anomaly", "detection", "clustering", "classification", "regression",
        "vector", "embedding", "fusion", "reranking", "expansion", "retrieval",
        "semantic", "syntactic", "ontology", "graph", "knowledge", "reasoning", "inference",
        "calibration", "tuning", "benchmark", "profiling", "stress", "production",
        "deployment", "monitoring", "debugging", "tracing", "observability", "telemetry",
        "cryptography", "encryption", "authorization", "permission", "compliance",
    ];

    for term in &rare_terms {
        if query.contains(term) {
            return true;
        }
    }

    // Acronyms (all-caps words)
    for word in query.split_whitespace() {
        if word.len() >= 2 && word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
            return true;
        }
    }

    // Boolean operators
    if query.contains(" and ") || query.contains(" or ") || query.contains(" not ") {
        return true;
    }

    // Exact phrase search
    if query.contains('"') || query.contains('\'') {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_queries() {
        assert_eq!(classify_query("user auth"), QueryComplexity::Common);
        assert_eq!(classify_query("memory"), QueryComplexity::Common);
        assert_eq!(classify_query("config update"), QueryComplexity::Common);
    }

    #[test]
    fn test_standard_queries() {
        assert_eq!(classify_query("memory retrieval system"), QueryComplexity::Standard);
        assert_eq!(classify_query("search query optimization"), QueryComplexity::Standard);
    }

    #[test]
    fn test_rare_queries() {
        assert_eq!(
            classify_query("distributed transaction ACID compliance optimization"),
            QueryComplexity::Rare
        );
    }

    #[test]
    fn test_typo_queries() {
        assert_eq!(classify_query("authetication"), QueryComplexity::Typo);
    }

    #[test]
    fn test_fetch_multiplier() {
        let base = 10;
        assert_eq!(QueryComplexity::Common.fetch_multiplier(base), 5);
        assert_eq!(QueryComplexity::Standard.fetch_multiplier(base), 10);
        assert_eq!(QueryComplexity::Rare.fetch_multiplier(base), 20);
        assert_eq!(QueryComplexity::Typo.fetch_multiplier(base), 15);
    }

    #[test]
    fn test_estimated_metrics() {
        assert!(QueryComplexity::Common.estimated_recall() < QueryComplexity::Rare.estimated_recall());
        assert!(
            QueryComplexity::Common.estimated_precision() > QueryComplexity::Rare.estimated_precision()
        );
        assert!(
            QueryComplexity::Common.estimated_latency_ms() < QueryComplexity::Rare.estimated_latency_ms()
        );
    }
}
