//! Smart concept creation with filtering and clustering
//!
//! Before creating concepts, filter out noise and group similar candidates
//! to avoid creating near-duplicates.

use anyhow::Result;
use crate::concept_extraction::ExtractedConcept;

/// Options for smart concept creation
pub struct SmartCreationOpts {
    /// Minimum confidence threshold (varies by type, but can override)
    pub min_confidence_override: Option<f32>,
    /// Cluster similar concepts (Jaro-Winkler >= threshold before creation)
    pub clustering_threshold: f32,
    /// Allow single-char or very short concepts
    pub allow_short: bool,
}

impl Default for SmartCreationOpts {
    fn default() -> Self {
        Self {
            min_confidence_override: None,
            clustering_threshold: 0.85,  // Don't create if similar to each other
            allow_short: false,
        }
    }
}

/// Result of smart concept filtering and clustering
#[derive(Debug, Clone)]
pub struct SmartCreationResult {
    /// Concepts approved for creation (one per cluster)
    pub approved: Vec<ExtractedConcept>,
    /// Concepts filtered out with reason
    pub filtered: Vec<(ExtractedConcept, String)>,
    /// Clustered concepts (similar ones grouped)
    pub clusters: Vec<Vec<ExtractedConcept>>,
}

/// Filter and cluster concepts before creation
///
/// 1. Filter out noise (stopwords, temporal, too short, low confidence)
/// 2. Group similar concepts (Jaro-Winkler >= threshold)
/// 3. Keep highest-confidence from each cluster
pub fn filter_and_cluster_concepts(
    candidates: Vec<ExtractedConcept>,
    opts: SmartCreationOpts,
) -> SmartCreationResult {
    let mut filtered = Vec::new();
    let mut approved_for_clustering = Vec::new();

    // Step 1: Filter candidates
    for candidate in candidates {
        match validate_concept(&candidate, &opts) {
            Ok(_) => approved_for_clustering.push(candidate),
            Err(reason) => filtered.push((candidate, reason)),
        }
    }

    // Step 2: Cluster remaining candidates
    let clusters = cluster_by_similarity(&approved_for_clustering, opts.clustering_threshold);

    // Step 3: Pick best from each cluster
    let approved: Vec<_> = clusters
        .iter()
        .filter_map(|cluster| {
            cluster
                .iter()
                .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
                .cloned()
        })
        .collect();

    SmartCreationResult {
        approved,
        filtered,
        clusters,
    }
}

/// Validate a single concept for creation
fn validate_concept(concept: &ExtractedConcept, opts: &SmartCreationOpts) -> Result<(), String> {
    // 1. Length check
    let word_count = concept.text.split_whitespace().count();
    if !opts.allow_short && word_count < 2 && concept.text.len() < 4 {
        return Err(format!("Too short: {} chars, {} words", concept.text.len(), word_count));
    }

    // 2. Noise checks
    if is_stopword(&concept.text) {
        return Err("Is stopword".to_string());
    }

    if is_temporal(&concept.text) {
        return Err("Is temporal (date/time)".to_string());
    }

    if is_abbreviation_only(&concept.text) {
        return Err("Is abbreviation only".to_string());
    }

    // 3. Confidence threshold
    let min_confidence = opts
        .min_confidence_override
        .unwrap_or_else(|| confidence_threshold_for_type(concept.concept_type.as_deref()));

    if concept.confidence < min_confidence {
        return Err(format!(
            "Low confidence: {:.2} < {:.2}",
            concept.confidence, min_confidence
        ));
    }

    Ok(())
}

/// Get minimum confidence threshold for a concept type
fn confidence_threshold_for_type(concept_type: Option<&str>) -> f32 {
    match concept_type {
        Some("TECHNIQUE") | Some("ARCHITECTURE") | Some("PATTERN") => 0.80,
        Some("METHODOLOGY") | Some("PRINCIPLE") => 0.75,
        Some("DOMAIN") | Some("PROCESS") => 0.70,
        Some("ENTITY") => 0.65,
        _ => 0.60,
    }
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
    text.len() <= 2 && text.chars().all(|c| c.is_ascii_uppercase() || !c.is_ascii_alphabetic())
}

/// Cluster concepts by Jaro-Winkler similarity
fn cluster_by_similarity(concepts: &[ExtractedConcept], threshold: f32) -> Vec<Vec<ExtractedConcept>> {
    let mut clusters: Vec<Vec<ExtractedConcept>> = Vec::new();

    for concept in concepts {
        let mut found_cluster = false;

        // Try to find existing cluster with similarity >= threshold
        for cluster in &mut clusters {
            if let Some(first) = cluster.first() {
                let similarity = strsim::jaro_winkler(
                    &concept.text.to_lowercase(),
                    &first.text.to_lowercase(),
                ) as f32;

                if similarity >= threshold {
                    cluster.push(concept.clone());
                    found_cluster = true;
                    break;
                }
            }
        }

        // Create new cluster if not found
        if !found_cluster {
            clusters.push(vec![concept.clone()]);
        }
    }

    clusters
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_concept(text: &str, confidence: f32, concept_type: Option<&str>) -> ExtractedConcept {
        ExtractedConcept {
            text: text.to_string(),
            sources: vec![],
            confidence,
            concept_type: concept_type.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_validate_concept_good() {
        let concept = make_concept("JWT token", 0.85, Some("TECHNIQUE"));
        let opts = SmartCreationOpts::default();
        assert!(validate_concept(&concept, &opts).is_ok());
    }

    #[test]
    fn test_validate_concept_too_short() {
        let concept = make_concept("a", 0.90, None);
        let opts = SmartCreationOpts::default();
        assert!(validate_concept(&concept, &opts).is_err());
    }

    #[test]
    fn test_validate_concept_stopword() {
        let concept = make_concept("the", 0.90, None);
        let opts = SmartCreationOpts::default();
        assert!(validate_concept(&concept, &opts).is_err());
    }

    #[test]
    fn test_validate_concept_temporal() {
        let concept = make_concept("2026", 0.90, None);
        let opts = SmartCreationOpts::default();
        assert!(validate_concept(&concept, &opts).is_err());
    }

    #[test]
    fn test_validate_concept_low_confidence() {
        let concept = make_concept("testing", 0.50, Some("TECHNIQUE"));
        let opts = SmartCreationOpts::default();
        assert!(validate_concept(&concept, &opts).is_err());
    }

    #[test]
    fn test_filter_and_cluster() {
        let concepts = vec![
            make_concept("microservice", 0.80, Some("ARCHITECTURE")),
            make_concept("microservices", 0.79, Some("ARCHITECTURE")),  // Similar but lower conf
            make_concept("the", 0.90, None),  // Stopword - should be filtered
            make_concept("2026", 0.85, None),  // Temporal - should be filtered
            make_concept("JWT", 0.92, Some("TECHNIQUE")),
        ];
        let opts = SmartCreationOpts::default();

        let result = filter_and_cluster_concepts(concepts, opts);

        // Check filtering: "the" and "2026" should be filtered
        let filtered_texts: Vec<String> = result.filtered.iter().map(|(c, _)| c.text.clone()).collect();
        assert!(filtered_texts.contains(&"the".to_string()));
        assert!(filtered_texts.contains(&"2026".to_string()));

        // Check approval: should have some approved concepts
        assert!(result.approved.len() > 0);
    }
}
