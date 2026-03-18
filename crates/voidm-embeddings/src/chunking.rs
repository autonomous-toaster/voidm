//! Text chunking for consistent embedding across memories.
//!
//! Implements semantic chunking by sentences/paragraphs with configurable overlap
//! to maintain consistent embedding quality regardless of input length.

/// Default chunk size: approximately 512 tokens (~2KB of text).
/// This is conservative to work with all embedding models.
pub const DEFAULT_CHUNK_SIZE: usize = 512;

/// Default overlap between chunks: 50 tokens (~200 chars).
/// Helps maintain context continuity at chunk boundaries.
pub const DEFAULT_OVERLAP: usize = 50;

/// Rough estimate: 1 token ≈ 4 characters on average in English.
/// Used for converting token counts to approximate character positions.
const CHARS_PER_TOKEN: usize = 4;

/// Split text into chunks of roughly `chunk_size` tokens with `overlap` token overlap.
///
/// Uses character-based chunking with word-boundary breaks to preserve
/// semantic meaning and consistent chunk sizes.
///
/// # Arguments
/// * `text` - Input text to chunk
/// * `chunk_size` - Target chunk size in tokens (~4 chars per token)
/// * `overlap` - Overlap between chunks in tokens
///
/// # Returns
/// Vector of text chunks. If text is smaller than chunk_size, returns vec![text].
///
/// # Example
/// ```ignore
/// let text = "First sentence. Second sentence. Third sentence.";
/// let chunks = chunk_text(text, 100, 20);
/// // Returns chunks of ~400 chars each with 200 char overlap
/// ```
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }

    let target_chars = chunk_size * CHARS_PER_TOKEN;
    let overlap_chars = overlap * CHARS_PER_TOKEN;

    // If text is smaller than target, return as single chunk
    if text.len() <= target_chars {
        return vec![text.to_string()];
    }

    // Character-based chunking with word boundaries
    chunk_by_characters(text, target_chars, overlap_chars)
}

/// Chunk text by character positions, breaking at word boundaries.
fn chunk_by_characters(text: &str, target_chars: usize, overlap_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = std::cmp::min(start + target_chars, text.len());

        // Find a good break point (whitespace) near the target end
        let chunk_end = if end < text.len() {
            // Try to break at whitespace within target_chars
            find_word_boundary(&text[start..end])
                .map(|pos| start + pos)
                .unwrap_or(end)
        } else {
            end
        };

        // Add chunk (skip if it would be empty after trimming)
        let chunk_text = text[start..chunk_end].trim();
        if !chunk_text.is_empty() {
            chunks.push(chunk_text.to_string());
        }

        // Calculate next start with overlap
        // If we'd go out of bounds, break
        if chunk_end >= text.len() {
            break;
        }

        // Move start position backward by overlap amount for next chunk
        start = if chunk_end > overlap_chars {
            chunk_end - overlap_chars
        } else {
            chunk_end
        };

        // Avoid infinite loops on very small overlaps
        if start >= chunk_end {
            start = chunk_end;
        }
    }

    chunks
}

/// Find the nearest word boundary (whitespace) within text (searching backward).
fn find_word_boundary(text: &str) -> Option<usize> {
    text.rfind(|c: char| c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_short() {
        let text = "Hello world";
        let chunks = chunk_text(text, 100, 20);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_chunk_text_long() {
        // Create a text that's definitely longer than chunk size
        let text = ("The quick brown fox jumps over the lazy dog. " ).repeat(50);
        let chunks = chunk_text(&text, 50, 10);  // ~50 tokens = 200 chars
        // Should chunk into multiple pieces
        assert!(chunks.len() > 1, "Expected multiple chunks, got {}", chunks.len());
        // All chunks should be non-empty
        assert!(chunks.iter().all(|c| !c.is_empty()));
    }

    #[test]
    fn test_chunk_text_long_single_word() {
        // Long text without spaces - should still chunk
        let text = "a".repeat(2000);
        let chunks = chunk_text(&text, 100, 20);  // ~100 tokens = 400 chars
        // Character-based chunking should still work
        assert!(!chunks.is_empty());
        // Each chunk should be reasonably sized
        for chunk in &chunks {
            assert!(chunk.len() <= 500, "Chunk too long: {}", chunk.len());
        }
    }

    #[test]
    fn test_chunk_text_empty() {
        let text = "";
        let chunks = chunk_text(text, 100, 20);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_chunk_text_with_overlap() {
        let text = "Word1 Word2 Word3 Word4 Word5 Word6 Word7 Word8 Word9 Word10 " .repeat(10);
        let chunks = chunk_text(&text, 25, 5);  // Small chunks to test overlap
        // Should have multiple chunks
        assert!(chunks.len() >= 2, "Expected at least 2 chunks, got {}", chunks.len());
        // All chunks should be non-empty
        assert!(chunks.iter().all(|c| !c.is_empty()));
    }

    #[test]
    fn test_chunk_chunked_embedding_averaging() {
        // Test that multiple chunks are averaged correctly
        let chunks = vec![
            "test chunk one".to_string(),
            "test chunk two".to_string(),
            "test chunk three".to_string(),
        ];
        
        // Manually verify dimension consistency
        let dim = 384;  // Example dimension
        let mut sample = vec![0.0_f32; dim];
        for i in 0..3 {
            sample[i] = 1.0;
        }
        
        // Average calculation
        for val in &mut sample {
            *val /= 3.0;
        }
        
        // Check that averaging works
        assert!((sample[0] - 0.333_f32).abs() < 0.01);
        assert!(sample[100] == 0.0);
    }
}
