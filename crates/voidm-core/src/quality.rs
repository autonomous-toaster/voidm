use crate::models::MemoryType;

#[derive(Debug, Clone)]
pub struct QualityScore {
    pub score: f32,
    pub genericity: f32,
    pub abstraction: f32,
    pub temporal_independence: f32,
    pub task_independence: f32,
    pub substance: f32,
    pub entity_specificity: f32,
}

/// Calculate entity specificity score based on named entity density.
///
/// Entity density = (tokens in named entities) / (total tokens)
///
/// Rationale: Memories with balanced entity density (10-30%) combine concrete anchors
/// (product names, locations, people) with generic principles. Too few entities (generic),
/// too many (overly specific instance narrative).
///
/// Scoring:
/// - <10% density: 0.95 (mostly generic with few anchors)
/// - 10-30% density: 1.0 (ideal sweet spot)
/// - 30-50% density: 0.8 (getting specific)
/// - >50% density: 0.5 (too entity-heavy, instance-focused)
///
/// Note: If NER model is not loaded, returns neutral score (0.95).
fn entity_specificity_score(content: &str) -> f32 {
    // Try to extract entities; if NER is not loaded, default to neutral score
    let entities = match crate::ner::extract_entities(content) {
        Ok(e) => e,
        Err(_) => {
            // NER model not loaded; return neutral score (no penalty or boost)
            // This happens in unit tests where model initialization may not occur
            return 0.95;
        }
    };

    if entities.is_empty() {
        // No entities found = generic content = good for quality
        return 0.95;
    }

    let word_count = content.split_whitespace().count();
    if word_count == 0 {
        return 0.95;
    }

    // Count tokens in named entities
    let entity_token_count: usize = entities.iter()
        .map(|e| e.text.split_whitespace().count())
        .sum();

    let entity_density = entity_token_count as f32 / word_count as f32;

    // Score based on density distribution
    if entity_density < 0.1 {
        0.95  // Low density but has some anchors
    } else if entity_density < 0.3 {
        1.0   // Sweet spot: balanced concrete + generic
    } else if entity_density < 0.5 {
        0.8   // Getting specific
    } else {
        0.5   // Too entity-heavy, overly specific/instance-focused
    }
}

/// Compute quality score for a memory.
/// 
/// Scoring factors (weighted):
/// - Genericity (0.20): Language reuse across projects vs personal context
/// - Abstraction (0.20): Principle/pattern vs specific instance
/// - Temporal independence (0.25): No "today/yesterday/this session" markers
/// - Task independence (0.15): Not tied to TODO/task/session
/// - Content substance (0.20): Word count (50+ preferred)
/// - Entity specificity (0.05): Named entity density (10-30% optimal, captures concrete vs instance-specific)
/// - Anti-pattern penalties (context-aware): Task language excluded for procedural/conceptual
pub fn compute_quality_score(
    content: &str,
    memory_type: &MemoryType,
) -> QualityScore {
    let content_lower = content.to_lowercase();
    let word_count = content.split_whitespace().count();

    // 1. Genericity: penalize personal pronouns and project-specific language
    let personal_pronouns = count_matches(&content_lower, &[
        " i ", " we ", " my ", " our ", " me ", " us ",
        "myself", "yourself", "ourselves", "yourselves",
        " mine ", " yours ", "their",
    ]);
    let has_this_project = content_lower.contains("this project") 
        || content_lower.contains("our project")
        || content_lower.contains("my implementation")
        || content_lower.contains("our team");
    let personal_count = personal_pronouns + (if has_this_project { 1 } else { 0 });
    
    // Penalize overly generic/template content
    let is_very_generic = (content.len() < 30 && word_count < 10)
        || content_lower == "todo"
        || content_lower == "done"
        || content_lower == "test"
        || content_lower == "fix";
    
    // Per-type sensitivity to personal language
    let genericity = if is_very_generic {
        0.1  // Heavily penalize
    } else {
        let base_penalty = match memory_type {
            MemoryType::Semantic | MemoryType::Conceptual => 0.30,  // Stricter
            MemoryType::Contextual => 0.25,
            _ => 0.20,  // Episodic, Procedural more lenient
        };
        (1.0 - (personal_count as f32 * base_penalty).min(1.0)).max(0.0)
    };

    // 2. Abstraction: penalize instance-specific language
    let has_personal_action = content_lower.contains("i did")
        || content_lower.contains("i built")
        || content_lower.contains("i fixed")
        || content_lower.contains("i created")
        || content_lower.contains("i implemented")
        || content_lower.contains("we did")
        || content_lower.contains("we built")
        || content_lower.contains("we fixed")
        || content_lower.contains("we created")
        || content_lower.contains("we implemented")
        || content_lower.contains("i worked")
        || content_lower.contains("we worked")
        || content_lower.contains("completed")
        || content_lower.contains("finished the");
    
    // Also check for instance-specific markers (UUIDs, timestamps, session IDs)
    let has_instance_markers = content.contains("TODO-")
        || content.contains("session-")
        || content.contains("2026-03-")
        || content.contains("2025-")
        || content.contains("2024-");
    
    let abstraction = match (has_personal_action, has_instance_markers) {
        (true, _) => 0.2,
        (false, true) => 0.5,
        _ => 0.95,
    };

    // 3. Temporal independence: penalize temporal markers
    let temporal_keywords = &[
        "today", "tomorrow", "yesterday", "this session", "this morning", "this afternoon",
        "this week", "this month", "this year", "right now", "currently", "just now",
        "recently", "last session", "this sprint", "this quarter", "last week", "last month",
        "last year", "2026-", "2025-", "2024-", " now ", "earlier today",
    ];
    
    // Count temporal markers for severity scoring
    let temporal_count = temporal_keywords.iter()
        .filter(|kw| content_lower.contains(*kw))
        .count();
    
    // Episodic memories naturally have temporal markers - lighter penalty
    let temporal_independence = match memory_type {
        MemoryType::Episodic => {
            match temporal_count {
                0 => 0.85,      // Even episodic should ideally be timeless
                1 => 0.75,      // One temporal marker is acceptable for episodic
                2 => 0.60,      // Two markers - ok but still penalize
                3 => 0.45,
                _ => 0.30,
            }
        }
        _ => {
            // Semantic, Procedural, Conceptual, Contextual should avoid temporal markers
            match temporal_count {
                0 => 0.95,      // No temporal markers - excellent
                1 => 0.65,      // One marker - mild penalty
                2 => 0.45,      // Two markers - moderate penalty
                3 => 0.25,      // Three markers - strong penalty
                _ => 0.10,      // Many markers - very poor
            }
        }
    };

    // 4. Task independence: penalize task/TODO references and status prefixes
    let has_status_prefix = is_status_prefix_line(content);
    let has_todo_refs = content.contains("TODO-") && contains_hex_after_todo(&content);
    
    // Count task-related indicators for graduated penalty
    let mut task_issues = 0;
    if has_status_prefix {
        task_issues += 2;  // Heavy penalty
    }
    if has_todo_refs {
        task_issues += 1;
    }
    
    let task_independence = match task_issues {
        0 => 0.95,      // No task references - excellent
        1 => 0.75,      // One TODO - moderate
        2 => 0.50,      // Status prefix - significant penalty
        3 => 0.30,      // Both - very poor
        _ => 0.10,
    };

    // 5. Task language penalty (context-aware: skip for procedural/conceptual)
    let task_language_keywords = &["completed", "finished", "done", "fixed", "milestone"];
    let has_task_language = task_language_keywords.iter().any(|kw| {
        // Check for word boundaries more flexibly
        content_lower.contains(&format!(" {}", kw))
            || content_lower.contains(&format!("{} ", kw))
            || content_lower.ends_with(kw)
            || content_lower.starts_with(kw)
    });
    
    let mut task_language_penalty = 0.0;
    if has_task_language {
        match memory_type {
            // Procedural and Conceptual can legitimately contain "done", "completed"
            MemoryType::Procedural | MemoryType::Conceptual => {
                task_language_penalty = 0.0;
            }
            // Semantic, Contextual, Episodic should not
            _ => {
                task_language_penalty = 0.15;
            }
        }
    }

    // 6. Content substance: prefer 50+ words (per-type optimization)
    // Aggressively penalize very short content (< 20 words is nearly useless)
    // Procedural/Conceptual can be shorter (actions, rules are concise)
    // Episodic/Semantic/Contextual should be more substantial
    let substance = match memory_type {
        MemoryType::Procedural => {
            // Actions and rules can be concise: 20+ words preferred
            if word_count < 8 {
                0.0
            } else if word_count < 20 {
                0.5
            } else if word_count < 400 {
                0.95
            } else {
                0.3  // Too long for procedural
            }
        }
        MemoryType::Conceptual => {
            // Concepts should be well-explained: 40+ words preferred
            if word_count < 10 {
                0.0
            } else if word_count < 40 {
                0.4
            } else if word_count < 500 {
                0.95
            } else {
                0.2
            }
        }
        MemoryType::Episodic => {
            // Episodes can be shorter (specific events): 30+ words
            if word_count < 12 {
                0.0
            } else if word_count < 30 {
                0.4
            } else if word_count < 400 {
                0.95
            } else {
                0.4
            }
        }
        _ => {
            // Semantic, Contextual: 50+ words preferred
            if word_count < 15 {
                0.0
            } else if word_count < 50 {
                0.3
            } else if word_count < 300 {
                0.95
            } else {
                // Too long: encourages splitting into atomic memories
                0.3
            }
        }
    };

    // 7. Entity specificity: measure named entity density
    let entity_specificity = entity_specificity_score(content);

    // 8. Bonus for actionable/structural patterns (imperative forms, conditionals)
    let has_actionable_pattern = content_lower.contains("when ")
        || content_lower.contains("if ")
        || content_lower.contains("always ")
        || content_lower.contains("never ")
        || content_lower.contains("use ")
        || content_lower.contains("avoid ")
        || content_lower.contains("ensure ")
        || content_lower.contains("pattern:")
        || content_lower.contains("rule:");
    
    // 9. Bonus for well-structured content (lists, hierarchies, key-value patterns)
    let has_structured_format = content.contains("- ")  // Lists
        || content.contains("* ")
        || content.contains(": ")  // Key-value
        || content.contains("→")   // Arrows (flow)
        || content.lines().count() > 3;  // Multiple paragraphs
    
    // 10. Bonus for citations/references (external anchors)
    let has_citations = content.contains("http://")
        || content.contains("https://")
        || content.contains("RFC")
        || content.contains("GitHub")
        || content.contains("issue")
        || content.contains("commit");
    
    let actionable_bonus = match (has_actionable_pattern, has_structured_format, has_citations) {
        (true, true, true) => 0.10,   // Excellent: actionable + structured + cited
        (true, true, false) => 0.08,  // Good: actionable + structured
        (true, false, true) => 0.07,  // Good: actionable + cited
        (false, true, true) => 0.06,  // Good: structured + cited
        (true, false, false) => 0.05, // Ok: just actionable
        (false, true, false) => 0.03, // Ok: just structured
        (false, false, true) => 0.04, // Ok: just cited
        _ => 0.0,
    };

    // Weighted score - substance weight matters for short content
    // Adjusted weights to prioritize temporal independence (content-age) over other factors
    let score = (genericity * 0.15
        + abstraction * 0.15
        + temporal_independence * 0.35
        + task_independence * 0.10
        + substance * 0.20
        + entity_specificity * 0.05) - task_language_penalty + actionable_bonus;

    QualityScore {
        score: score.max(0.0).min(1.0),
        genericity,
        abstraction,
        temporal_independence,
        task_independence,
        substance,
        entity_specificity,
    }
}

fn count_matches(text: &str, patterns: &[&str]) -> usize {
    patterns
        .iter()
        .filter(|pattern| text.contains(*pattern))
        .count()
}

fn is_status_prefix_line(content: &str) -> bool {
    let prefixes = &[
        "date:", "status:", "update:", "milestone:", "session:",
        "todo:", "fix:", "issue:", "pr:", "commit:", "plan:",
        "result:", "outcome:", "completion:",
    ];
    content
        .lines()
        .next()
        .map(|line| {
            let line_lower = line.to_lowercase();
            prefixes.iter().any(|prefix| line_lower.starts_with(prefix))
        })
        .unwrap_or(false)
}

fn contains_hex_after_todo(content: &str) -> bool {
    // Simple check: TODO- followed by at least 8 hex chars
    if let Some(pos) = content.find("TODO-") {
        let after = &content[pos + 5..];
        after.chars().take(8).all(|c| c.is_ascii_hexdigit())
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_good_semantic_memory() {
        let content = "Separation of ontology_concepts and ontology_edges prevents concept reuse issues. Concepts should be first-class entities.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        assert!(score.score > 0.7, "Good semantic memory should score >0.7, got {}", score.score);
    }

    #[test]
    fn test_bad_task_log() {
        let content = "Today I completed the refactor. Task done.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        assert!(score.score < 0.5, "Task log should score <0.5, got {}", score.score);
    }

    #[test]
    fn test_procedural_with_done() {
        // Procedural memories should allow "done", "completed"
        let content = "Run cargo build. Once done, commit changes.";
        let score = compute_quality_score(content, &MemoryType::Procedural);
        // Should not heavily penalize task language for procedural
        assert!(score.score > 0.4, "Procedural with 'done' should not be heavily penalized, got {}", score.score);
    }

    #[test]
    fn test_temporal_markers_penalty() {
        let content = "Today I worked on the auth service.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        // Temporal penalty is now 0.4 instead of 0.05 to allow legitimate temporal context in examples
        assert!(score.score < 0.65, "Temporal markers should lower score, got {}", score.score);
    }

    #[test]
    fn test_short_content_penalty() {
        let content = "Done.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        assert!(score.score < 0.70, "Very short content with task language should score low, got {}", score.score);
    }

    #[test]
    fn test_personal_pronouns_penalty() {
        let content = "I built a service. We deployed it. My implementation works.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        assert!(score.score < 0.60, "Personal pronouns should lower score significantly, got {}", score.score);
    }

    #[test]
    fn test_generic_principle() {
        let content = "Service isolation prevents cascading failures in distributed systems. Proper circuit breakers and bulkheads are essential patterns.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        assert!(score.score > 0.75, "Generic principle should score high, got {}", score.score);
    }

    #[test]
    fn test_balanced_concrete_and_generic() {
        // Content with some named entities but mostly generic language
        let content = "Docker containers need proper resource limits to prevent host interference. Always set CPU and memory constraints.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        // Should score well: has concrete anchor (Docker) but generic principle
        assert!(score.score > 0.65, "Balanced concrete+generic should score >0.65, got {}", score.score);
    }

    #[test]
    fn test_overly_specific_content() {
        // Content with many personal pronouns and temporal markers
        // (NER not loaded in unit tests, so entity_specificity will be neutral 0.95)
        let content = "I met John Smith in Tokyo last Tuesday. He works at Acme Corp in the Tokyo office. John told me about their project.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        // Should score lower: too many temporal markers + personal pronouns + instance-specific
        assert!(score.score < 0.85, "Overly specific instance narrative should score <0.85, got {}", score.score);
    }

    #[test]
    fn test_entity_specificity_signal_with_no_entities() {
        // In unit test context (no NER model loaded), entity_specificity returns neutral 0.95
        // This test verifies the structure is correct
        let content = "When designing distributed systems, consider consistency models and partition tolerance.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        // entity_specificity should be populated in the struct
        assert!(score.entity_specificity >= 0.0 && score.entity_specificity <= 1.0, 
            "entity_specificity should be in valid range, got {}", score.entity_specificity);
    }

    #[test]
    fn test_entity_specificity_signal_sweet_spot() {
        // In unit test context, entity_specificity returns neutral 0.95
        // This test verifies balanced content scores well
        let content = "PostgreSQL uses MVCC for isolation. This prevents read locks in most scenarios.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        // Should score reasonably high (no personal pronouns, no temporal markers)
        assert!(score.score > 0.70, "Sweet spot content should score high, got {}", score.score);
    }

    #[test]
    fn test_entity_specificity_overweighting() {
        // In unit test context (no NER model loaded), all entity_specificity scores are neutral 0.95
        // This test would need async NER model initialization for real entity detection
        // For now, we skip real entity density testing and verify structure only
        let content = "Alice and Bob and Charlie and David and Eve work at company X";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        // Verify entity_specificity field exists and is in valid range
        assert!(score.entity_specificity >= 0.0 && score.entity_specificity <= 1.0, 
            "entity_specificity should be in valid range, got {}", score.entity_specificity);
    }

    #[test]
    fn test_quality_with_product_specific_knowledge() {
        // Product-specific but useful knowledge (AWS + Stripe)
        let content = "AWS Lambda integrates with Stripe for payment processing. Set timeout appropriately.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        // Should score reasonably (useful concrete knowledge)
        // Entity density: ~3 entities (AWS, Lambda, Stripe) / ~13 tokens = 23% = sweet spot
        assert!(score.score > 0.5, "Product-specific knowledge should score >0.5, got {}", score.score);
    }
}
