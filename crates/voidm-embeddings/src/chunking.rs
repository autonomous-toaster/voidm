//! Text chunking utilities for embeddings and retrieval.
//!
//! Canonical chunking entry point for the codebase.
//! Supports smart semantic chunking with configurable bounds and overlap,
//! plus conversion to memory-owned chunks with deterministic IDs.

use uuid::Uuid;

/// Default chunk size in characters.
/// Tuned for embedding quality and bounded retrieval/context assembly.
pub const DEFAULT_CHUNK_SIZE: usize = 600;

/// Default overlap between chunks in characters.
/// Helps maintain context continuity at chunk boundaries.
pub const DEFAULT_OVERLAP: usize = 100;

/// Chunking configuration.
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    pub target_size: usize,
    pub min_chunk_size: usize,
    pub max_chunk_size: usize,
    pub overlap: usize,
    pub smart_breaks: bool,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            target_size: DEFAULT_CHUNK_SIZE,
            min_chunk_size: 150,
            max_chunk_size: 900,
            overlap: DEFAULT_OVERLAP,
            smart_breaks: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakType {
    Paragraph,
    Sentence,
    Word,
    Character,
}

#[derive(Debug, Clone)]
pub struct OwnedChunk {
    pub id: String,
    pub memory_id: String,
    pub index: usize,
    pub content: String,
    pub size: usize,
    pub break_type: BreakType,
    pub created_at: String,
}

impl OwnedChunk {
    pub fn new(memory_id: &str, index: usize, content: String, break_type: BreakType, created_at: String) -> Self {
        Self {
            id: generate_chunk_id(memory_id, index),
            memory_id: memory_id.to_string(),
            index,
            size: content.len(),
            content,
            break_type,
            created_at,
        }
    }
}

fn generate_chunk_id(memory_id: &str, index: usize) -> String {
    let namespace = Uuid::NAMESPACE_OID;
    let name = format!("{}:chunk:{}", memory_id, index);
    let chunk_uuid = Uuid::new_v5(&namespace, name.as_bytes());
    format!("mchk_{}", chunk_uuid.simple())
}

#[derive(Debug, Clone)]
struct ChunkPart {
    content: String,
    break_type: BreakType,
}

/// Canonical chunking API returning plain text chunks.
pub fn chunk_text(text: &str, config: &ChunkingConfig) -> Vec<String> {
    chunk_text_parts(text, config)
        .into_iter()
        .map(|p| p.content)
        .collect()
}

/// Chunk and attach deterministic memory-owned metadata.
pub fn chunk_memory(memory_id: &str, text: &str, created_at: &str, config: &ChunkingConfig) -> Vec<OwnedChunk> {
    chunk_text_parts(text, config)
        .into_iter()
        .enumerate()
        .map(|(index, part)| OwnedChunk::new(memory_id, index, part.content, part.break_type, created_at.to_string()))
        .collect()
}

fn chunk_text_parts(text: &str, config: &ChunkingConfig) -> Vec<ChunkPart> {
    if text.trim().is_empty() {
        return vec![];
    }

    let mut chunks = if !config.smart_breaks {
        chunk_by_characters(text, config.target_size, config.overlap)
            .into_iter()
            .map(|content| ChunkPart { content, break_type: BreakType::Character })
            .collect()
    } else {
        chunk_smart_text(text, config)
    };

    merge_small_trailing_chunk(&mut chunks, config.min_chunk_size);
    chunks
}

fn chunk_smart_text(text: &str, config: &ChunkingConfig) -> Vec<ChunkPart> {
    for (delim, break_type) in [
        ("\n\n", BreakType::Paragraph),
        (". ", BreakType::Sentence),
        (" ", BreakType::Word),
    ] {
        let chunks = try_chunk_by_delim(text, config, delim, break_type);
        if is_valid_chunking(&chunks, config) {
            return with_overlap(chunks, config.overlap, config.max_chunk_size);
        }
    }

    chunk_by_characters(text, config.target_size, config.overlap)
        .into_iter()
        .map(|content| ChunkPart { content, break_type: BreakType::Character })
        .collect()
}

fn try_chunk_by_delim(text: &str, config: &ChunkingConfig, delim: &str, break_type: BreakType) -> Vec<ChunkPart> {
    let parts: Vec<&str> = text.split(delim).collect();
    let mut chunks = Vec::new();
    let mut current = String::new();

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let candidate = if current.is_empty() {
            part.to_string()
        } else {
            format!("{}{}{}", current, delim, part)
        };

        if candidate.len() > config.target_size && !current.is_empty() {
            chunks.push(ChunkPart { content: current, break_type });
            current = part.to_string();
        } else if candidate.len() > config.max_chunk_size {
            return Vec::new();
        } else {
            current = candidate;
        }
    }

    if !current.is_empty() {
        chunks.push(ChunkPart { content: current, break_type });
    }

    chunks
}

fn is_valid_chunking(chunks: &[ChunkPart], config: &ChunkingConfig) -> bool {
    !chunks.is_empty()
        && chunks.iter().all(|c| c.content.len() >= config.min_chunk_size && c.content.len() <= config.max_chunk_size)
}

fn with_overlap(chunks: Vec<ChunkPart>, overlap: usize, max_chunk_size: usize) -> Vec<ChunkPart> {
    if overlap == 0 || chunks.len() <= 1 {
        return chunks;
    }

    let mut out = Vec::with_capacity(chunks.len());
    for (idx, chunk) in chunks.iter().enumerate() {
        if idx == 0 {
            out.push(chunk.clone());
            continue;
        }

        let prev = &chunks[idx - 1].content;
        let prev_chars: Vec<char> = prev.chars().collect();
        let overlap_start_char = prev_chars.len().saturating_sub(overlap);
        let prefix: String = prev_chars[overlap_start_char..].iter().collect();
        let mut combined = format!("{}{}", prefix, chunk.content);
        if combined.chars().count() > max_chunk_size {
            combined = combined.chars().take(max_chunk_size).collect();
        }
        out.push(ChunkPart {
            content: combined,
            break_type: chunk.break_type,
        });
    }
    out
}

fn merge_small_trailing_chunk(chunks: &mut Vec<ChunkPart>, min_chunk_size: usize) {
    if chunks.len() < 2 {
        return;
    }

    let last_too_small = chunks.last().map(|c| c.content.len() < min_chunk_size).unwrap_or(false);
    if last_too_small {
        let last = chunks.pop().unwrap();
        if let Some(prev) = chunks.last_mut() {
            prev.content.push_str("\n\n");
            prev.content.push_str(&last.content);
        }
    }
}

/// Chunk text by character positions, breaking at word boundaries.
fn chunk_by_characters(text: &str, target_chars: usize, overlap_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if chars.is_empty() {
        return chunks;
    }

    let total_chars = text.chars().count();
    let mut start_char = 0usize;

    while start_char < total_chars {
        let end_char = std::cmp::min(start_char + target_chars, total_chars);
        let start_byte = chars[start_char].0;
        let end_byte = if end_char < total_chars { chars[end_char].0 } else { text.len() };

        let chunk_end_byte = if end_char < total_chars {
            find_word_boundary(&text[start_byte..end_byte]).map(|pos| start_byte + pos).unwrap_or(end_byte)
        } else {
            end_byte
        };

        let chunk_text = text[start_byte..chunk_end_byte].trim();
        if !chunk_text.is_empty() {
            chunks.push(chunk_text.to_string());
        }

        if chunk_end_byte >= text.len() {
            break;
        }

        let chunk_end_char = text[..chunk_end_byte].chars().count();
        start_char = if chunk_end_char > overlap_chars { chunk_end_char - overlap_chars } else { chunk_end_char };
        if start_char >= chunk_end_char {
            start_char = chunk_end_char;
        }
    }

    chunks
}

fn find_word_boundary(text: &str) -> Option<usize> {
    text.rfind(|c: char| c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_short() {
        let chunks = chunk_text("Hello world", &ChunkingConfig::default());
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_memory_has_ids() {
        let chunks = chunk_memory("mem-1", "A somewhat longer piece of text that should still produce at least one chunk.", "2026-01-01T00:00:00Z", &ChunkingConfig::default());
        assert!(!chunks.is_empty());
        assert!(chunks[0].id.starts_with("mchk_"));
    }
}
