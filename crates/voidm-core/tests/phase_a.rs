// Integration test for Phase A modules

#[cfg(test)]
mod tests {
    use voidm_core::chunking::{chunk_smart, ChunkingStrategy, BreakType};
    use voidm_core::validation::{validate_memory_length, MEMORY_SOFT_LIMIT, MEMORY_HARD_LIMIT};
    use voidm_core::coherence::{estimate_coherence, CoherenceScore};

    #[test]
    fn test_phase_a_chunking() {
        let strategy = ChunkingStrategy::default();
        // Use content large enough to require multiple chunks
        let content = "First paragraph with more content to make it longer. This paragraph should be substantial enough to test chunking properly.\n\nSecond paragraph with more details and additional content. This should also be large enough to trigger chunking behavior.\n\nThird paragraph with additional information and context. Making sure it's long enough to push past the target size.\n\nFourth paragraph to ensure multiple chunks. More text here.";
        let chunks = chunk_smart(content, &strategy).unwrap();
        
        println!("Content length: {} chars", content.len());
        println!("Chunks: {}", chunks.len());
        for chunk in &chunks {
            println!("  - [{}] {} chars: {}", chunk.index, chunk.size, &chunk.content[..30.min(chunk.content.len())]);
        }
        
        assert!(chunks.len() >= 1, "Should have at least one chunk");
        assert!(chunks.iter().all(|c| c.size > 0), "All chunks should have content");
    }

    #[test]
    fn test_phase_a_validation() {
        // Short memory
        let result = validate_memory_length("short").unwrap();
        assert!(result.is_within_hard_limit);
        assert!(!result.is_within_target);
        assert!(result.warning_message.is_none());
        
        // Optimal memory
        let content = "a".repeat(5000);
        let result = validate_memory_length(&content).unwrap();
        assert!(result.is_within_soft_limit);
        assert!(result.is_within_target);
        
        // Over soft limit
        let content = "a".repeat(15000);
        let result = validate_memory_length(&content).unwrap();
        assert!(!result.is_within_soft_limit);
        assert!(result.warning_message.is_some());
        
        // Over hard limit
        let content = "a".repeat(60000);
        let result = validate_memory_length(&content);
        assert!(result.is_err());
        
        println!("✓ Validation working correctly");
    }

    #[test]
    fn test_phase_a_coherence() {
        let score = estimate_coherence("First sentence. Second sentence. Third sentence.");
        let final_score = score.final_score();
        
        println!("Coherence: {:.2} {}", final_score, score.quality_level());
        assert!(final_score >= 0.0 && final_score <= 1.0);
        assert!(!score.quality_level().is_empty());
    }

    #[test]
    fn test_phase_a_integration() {
        // Simulate Phase A flow
        let memory = "OAuth2 is an authorization protocol.\n\nIt uses bearer tokens.\n\nTokens are validated on each request.";
        
        // 1. Validate length
        let validation = validate_memory_length(memory).unwrap();
        assert!(validation.is_within_hard_limit);
        println!("✓ Length validation: {} chars", validation.content_length);
        
        // 2. Chunk memory
        let strategy = ChunkingStrategy::default();
        let chunks = chunk_smart(memory, &strategy).unwrap();
        println!("✓ Chunked into {} chunks", chunks.len());
        
        // 3. Score each chunk
        for chunk in &chunks {
            let score = estimate_coherence(&chunk.content);
            println!("  Chunk {}: {:.2} {}", chunk.index, score.final_score(), score.quality_level());
        }
    }
}
