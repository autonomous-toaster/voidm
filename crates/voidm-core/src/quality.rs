use crate::models::MemoryType;

#[derive(Debug, Clone)]
pub struct QualityScore {
    pub score: f32,
    pub genericity: f32,
    pub abstraction: f32,
    pub temporal_independence: f32,
    pub task_independence: f32,
    pub substance: f32,
}

/// Compute quality score for a memory.
/// 
/// Scoring factors (weighted):
/// - Genericity (0.30): Language reuse across projects vs personal context
/// - Abstraction (0.20): Principle/pattern vs specific instance
/// - Temporal independence (0.20): No "today/yesterday/this session" markers
/// - Task independence (0.15): Not tied to TODO/task/session
/// - Content substance (0.10): Word count (50+ preferred)
/// - Anti-pattern penalties (context-aware): Task language excluded for procedural/conceptual
pub fn compute_quality_score(
    content: &str,
    memory_type: &MemoryType,
) -> QualityScore {
    let content_lower = content.to_lowercase();
    let word_count = content.split_whitespace().count();

    // 1. Genericity: penalize personal pronouns and project-specific language
    let personal_pronouns = count_matches(&content_lower, &[" i ", " we ", " my ", " our ", " me ", " us "]);
    let has_this_project = content_lower.contains("this project");
    let personal_count = personal_pronouns + (if has_this_project { 1 } else { 0 });
    let genericity = (1.0 - (personal_count as f32 * 0.25).min(1.0)).max(0.0);

    // 2. Abstraction: penalize instance-specific language
    let has_personal_action = content_lower.contains("i did")
        || content_lower.contains("i built")
        || content_lower.contains("i fixed")
        || content_lower.contains("we did")
        || content_lower.contains("we built")
        || content_lower.contains("we fixed");
    let abstraction = if has_personal_action { 0.2 } else { 0.95 };

    // 3. Temporal independence: penalize temporal markers
    let temporal_keywords = &[
        "today", "tomorrow", "yesterday", "this session", "this morning", "this afternoon",
        "this week", "this month", "this year", "right now",
    ];
    let has_temporal = temporal_keywords.iter().any(|kw| content_lower.contains(kw));
    // Penalty: 0.4 instead of 0.05 to allow some legitimate temporal context in examples
    let temporal_independence = if has_temporal { 0.4 } else { 0.95 };

    // 4. Task independence: penalize task/TODO references and status prefixes
    let has_status_prefix = is_status_prefix_line(content);
    let has_todo_refs = content.contains("TODO-") && contains_hex_after_todo(&content);
    
    let mut task_independence: f32 = 0.95;
    if has_status_prefix {
        task_independence -= 0.3;
    }
    if has_todo_refs {
        task_independence -= 0.2;
    }
    task_independence = task_independence.max(0.0);

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

    // 6. Content substance: prefer 50+ words
    // Aggressively penalize very short content (< 20 words is nearly useless)
    let substance = if word_count < 15 {
        0.0
    } else if word_count < 50 {
        0.3
    } else if word_count < 300 {
        0.95
    } else {
        // Too long: encourages splitting into atomic memories
        0.3
    };

    // Weighted score - substance weight matters for short content
    let score = (genericity * 0.20
        + abstraction * 0.20
        + temporal_independence * 0.25
        + task_independence * 0.15
        + substance * 0.20) - task_language_penalty;

    QualityScore {
        score: score.max(0.0).min(1.0),
        genericity,
        abstraction,
        temporal_independence,
        task_independence,
        substance,
    }
}

fn count_matches(text: &str, patterns: &[&str]) -> usize {
    patterns
        .iter()
        .filter(|pattern| text.contains(*pattern))
        .count()
}

fn is_status_prefix_line(content: &str) -> bool {
    let prefixes = &["date:", "status:", "update:", "milestone:"];
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
        assert!(score.score < 0.65, "Very short content with task language should score low, got {}", score.score);
    }

    #[test]
    fn test_personal_pronouns_penalty() {
        let content = "I built a service. We deployed it. My implementation works.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        assert!(score.score < 0.55, "Personal pronouns should lower score significantly, got {}", score.score);
    }

    #[test]
    fn test_generic_principle() {
        let content = "Service isolation prevents cascading failures in distributed systems. Proper circuit breakers and bulkheads are essential patterns.";
        let score = compute_quality_score(content, &MemoryType::Semantic);
        assert!(score.score > 0.75, "Generic principle should score high, got {}", score.score);
    }
}

