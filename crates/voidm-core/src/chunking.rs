/// Smart semantic chunking with paragraph-aware fallback strategy.
///
/// Chunking Strategy (in priority order):
/// 1. Split by paragraphs (\n\n) - best for semantic coherence
/// 2. Fall back to sentences (". ") - if paragraphs too large
/// 3. Fall back to words - if sentences too large
/// 4. Character fallback - never split mid-word
///
/// This ensures coherent chunks that respect semantic boundaries.

use anyhow::{anyhow, Result};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ChunkingStrategy {
    /// Target chunk size in characters (250-500 recommended)
    pub target_size: usize,
    /// Minimum chunk size (avoid fragments)
    pub min_chunk_size: usize,
    /// Maximum chunk size (hard limit)
    pub max_chunk_size: usize,
    /// Use smart breaks (paragraph/sentence) vs naive splitting
    pub smart_breaks: bool,
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self {
            target_size: 350,
            min_chunk_size: 50,
            max_chunk_size: 1000,
            smart_breaks: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: String,     // Unique chunk ID (mchk_<uuid>)
    pub index: usize,
    pub content: String,
    pub size: usize,
    pub break_type: BreakType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakType {
    Paragraph,
    Sentence,
    Word,
    Character,
}

impl Chunk {
    /// Create a new chunk with generated UUID based on memory_id and index
    pub fn new(memory_id: &str, index: usize, content: String, break_type: BreakType) -> Self {
        let id = generate_chunk_id(memory_id, index);
        let size = content.len();
        Self {
            id,
            index,
            content,
            size,
            break_type,
        }
    }
}

/// Generate a deterministic UUID v5 for a chunk
///
/// Uses UUID v5 (namespace + deterministic) so that:
/// - Same (memory_id, index) always produces same chunk ID
/// - IDs are globally unique
/// - IDs can be regenerated from memory_id + index
///
/// # Example
/// ```
/// let id1 = generate_chunk_id("abc123", 0);
/// let id2 = generate_chunk_id("abc123", 0);
/// assert_eq!(id1, id2);  // Same input → same output
/// ```
fn generate_chunk_id(memory_id: &str, index: usize) -> String {
    // Use DNS namespace for consistency
    let namespace = Uuid::NAMESPACE_DNS;
    let name = format!("{}:chunk:{}", memory_id, index);
    let chunk_uuid = Uuid::new_v5(&namespace, name.as_bytes());
    format!("mchk_{}", chunk_uuid.simple())
}

/// Smart chunking with paragraph-aware fallback.
///
/// # Algorithm
/// 1. Try paragraph breaks (\n\n)
/// 2. If any paragraph > max_chunk_size, try sentence breaks
/// 3. If any sentence > max_chunk_size, try word breaks
/// 4. As fallback, character-level split (never mid-word)
///
/// # Example
/// ```
/// let strategy = ChunkingStrategy::default();
/// let chunks = chunk_smart("abc123", "Para 1.\n\nPara 2.", &strategy).unwrap();
/// // chunks = [Chunk{id: "mchk_...", content: "Para 1.", ...}, ...]
/// ```
pub fn chunk_smart(memory_id: &str, content: &str, strategy: &ChunkingStrategy) -> Result<Vec<Chunk>> {
    if content.is_empty() {
        return Ok(vec![]);
    }

    if !strategy.smart_breaks {
        // Naive window-based chunking (not recommended)
        return chunk_naive(memory_id, content, strategy);
    }

    // Try paragraph-based chunking first
    if let Ok(chunks) = try_chunk_by_delim(memory_id, content, strategy, "\n\n", BreakType::Paragraph) {
        if is_valid_chunking(&chunks, strategy) {
            return Ok(chunks);
        }
    }

    // Fall back to sentence-based chunking
    if let Ok(chunks) = try_chunk_by_delim(memory_id, content, strategy, ". ", BreakType::Sentence) {
        if is_valid_chunking(&chunks, strategy) {
            return Ok(chunks);
        }
    }

    // Fall back to word-based chunking
    if let Ok(chunks) = try_chunk_by_delim(memory_id, content, strategy, " ", BreakType::Word) {
        if is_valid_chunking(&chunks, strategy) {
            return Ok(chunks);
        }
    }

    // Final fallback: character-level (never split mid-word)
    chunk_by_characters(memory_id, content, strategy)
}

/// Try to chunk by a given delimiter, respecting size constraints.
fn try_chunk_by_delim(
    memory_id: &str,
    content: &str,
    strategy: &ChunkingStrategy,
    delim: &str,
    break_type: BreakType,
) -> Result<Vec<Chunk>> {
    let parts: Vec<&str> = content.split(delim).collect();
    if parts.is_empty() {
        return Err(anyhow!("No parts after split"));
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut chunk_index = 0;

    for part in parts.iter() {
        let part_trimmed = part.trim();
        if part_trimmed.is_empty() {
            continue; // Skip empty parts
        }

        let part_with_delim = if current_chunk.is_empty() {
            part_trimmed.to_string()
        } else {
            format!("{}{}{}", current_chunk, delim, part_trimmed)
        };

        // Check if adding this part exceeds target
        if part_with_delim.len() > strategy.target_size && !current_chunk.is_empty() {
            // Current chunk is full, save it
            if current_chunk.len() >= strategy.min_chunk_size {
                chunks.push(Chunk::new(memory_id, chunk_index, current_chunk.clone(), break_type));
                chunk_index += 1;
            }
            // Start new chunk with current part
            current_chunk = part_trimmed.to_string();
        } else if part_with_delim.len() > strategy.max_chunk_size {
            // Part itself exceeds max, can't fit
            return Err(anyhow!(
                "Part of {} chars exceeds max_chunk_size of {}",
                part_with_delim.len(),
                strategy.max_chunk_size
            ));
        } else {
            current_chunk = part_with_delim;
        }
    }

    // Don't forget the last chunk
    if !current_chunk.is_empty() {
        chunks.push(Chunk::new(memory_id, chunk_index, current_chunk, break_type));
    }

    if chunks.is_empty() {
        return Err(anyhow!("No valid chunks created"));
    }

    Ok(chunks)
}

/// Chunk by characters, respecting word boundaries.
fn chunk_by_characters(
    memory_id: &str,
    content: &str,
    strategy: &ChunkingStrategy,
) -> Result<Vec<Chunk>> {
    if content.is_empty() {
        return Ok(vec![]);
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let mut chunk_index = 0;

    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Try to fill up to target size
        let mut j = i;
        let mut chunk_len = 0;

        while j < chars.len() && chunk_len < strategy.target_size {
            chunk_len += chars[j].len_utf8();
            j += 1;
        }

        // Back up to word boundary (space)
        while j > i && !chars[j - 1].is_whitespace() && j < chars.len() {
            chunk_len -= chars[j - 1].len_utf8();
            j -= 1;
        }

        // If we couldn't find a word boundary, take what we have
        if j == i {
            j = (i + 1).min(chars.len());
        }

        let chunk_str: String = chars[i..j].iter().collect();
        let trimmed = chunk_str.trim().to_string();

        if trimmed.len() >= strategy.min_chunk_size {
            chunks.push(Chunk::new(memory_id, chunk_index, trimmed, BreakType::Character));
            chunk_index += 1;
        }

        i = j;
    }

    if chunks.is_empty() {
        // Fallback: just return the whole content
        chunks.push(Chunk::new(memory_id, 0, content.to_string(), BreakType::Character));
    }

    Ok(chunks)
}

/// Validate that all chunks respect size constraints.
fn is_valid_chunking(chunks: &[Chunk], strategy: &ChunkingStrategy) -> bool {
    chunks.iter().all(|c| {
        c.size >= strategy.min_chunk_size && c.size <= strategy.max_chunk_size
    })
}

/// Naive fixed-window chunking (fallback if smart breaks fail).
fn chunk_naive(memory_id: &str, content: &str, strategy: &ChunkingStrategy) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let target = strategy.target_size;

    for (i, chunk) in content
        .as_bytes()
        .chunks(target)
        .enumerate()
    {
        let chunk_str = String::from_utf8_lossy(chunk).to_string();
        let trimmed = chunk_str.trim().to_string();
        if !trimmed.is_empty() && trimmed.len() >= strategy.min_chunk_size {
            chunks.push(Chunk::new(memory_id, i, trimmed, BreakType::Character));
        }
    }

    if chunks.is_empty() {
        chunks.push(Chunk::new(memory_id, 0, content.to_string(), BreakType::Character));
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_by_paragraphs() {
        let strategy = ChunkingStrategy::default();
        let content = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let chunks = chunk_smart(content, &strategy).unwrap();
        
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].break_type, BreakType::Paragraph);
        assert!(chunks[0].content.contains("First"));
    }

    #[test]
    fn test_chunk_long_paragraph() {
        let mut strategy = ChunkingStrategy::default();
        strategy.target_size = 100;
        
        let content = "This is a very long paragraph that should be broken into smaller pieces. It contains many words and should be split at word boundaries to preserve readability.";
        let chunks = chunk_smart(content, &strategy).unwrap();
        
        assert!(chunks.len() > 1);
        // Should use sentence or word breaks, not character
        assert!(!chunks.iter().all(|c| c.break_type == BreakType::Character));
    }

    #[test]
    fn test_chunk_respects_min_size() {
        let strategy = ChunkingStrategy::default();
        let content = "Short.\n\nAnother.";
        let chunks = chunk_smart(content, &strategy).unwrap();
        
        // Very short content might not produce chunks if below min
        for chunk in chunks {
            assert!(chunk.size >= strategy.min_chunk_size);
        }
    }

    #[test]
    fn test_chunk_respects_max_size() {
        let strategy = ChunkingStrategy::default();
        let content = "First.\n\nSecond.\n\nThird.";
        let chunks = chunk_smart(content, &strategy).unwrap();
        
        for chunk in chunks {
            assert!(chunk.size <= strategy.max_chunk_size);
        }
    }

    #[test]
    fn test_chunk_code_block() {
        let strategy = ChunkingStrategy::default();
        let content = "def hello():\n    print('world')\n    return True";
        let chunks = chunk_smart(content, &strategy).unwrap();
        
        assert!(!chunks.is_empty());
        // Should preserve code integrity
        let full_text = chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join(" ");
        assert!(full_text.contains("def hello"));
        assert!(full_text.contains("print"));
    }

    #[test]
    fn test_chunk_empty_content() {
        let strategy = ChunkingStrategy::default();
        let chunks = chunk_smart("", &strategy).unwrap();
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_chunk_single_paragraph() {
        let strategy = ChunkingStrategy::default();
        let content = "Single paragraph with just one section.";
        let chunks = chunk_smart(content, &strategy).unwrap();
        
        assert!(chunks.len() >= 1);
        assert_eq!(chunks[0].content.trim(), content.trim());
    }

    #[test]
    fn test_chunk_with_sentences() {
        let mut strategy = ChunkingStrategy::default();
        strategy.target_size = 50;
        
        let content = "First sentence. Second sentence. Third sentence.";
        let chunks = chunk_smart(content, &strategy).unwrap();
        
        assert!(chunks.len() > 1);
        // Should use sentence breaks if paragraphs unavailable
        let uses_sentences = chunks.iter().any(|c| c.break_type == BreakType::Sentence);
        assert!(uses_sentences || chunks[0].break_type == BreakType::Word);
    }
}
