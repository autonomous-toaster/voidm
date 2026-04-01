/// Shared memory sizing and chunking policy.
///
/// Centralizes limits used for validation, scoring, and retrieval-oriented chunking.

pub const MEMORY_WARNING_LENGTH: usize = 2_500;
pub const MEMORY_HARD_LIMIT: usize = 15_000;

pub const CHUNK_TARGET_SIZE: usize = 600;
pub const CHUNK_MIN_SIZE: usize = 150;
pub const CHUNK_MAX_SIZE: usize = 900;
pub const CHUNK_OVERLAP: usize = 100;

pub const RETRIEVAL_MAX_CHUNKS: usize = 6;
pub const RETRIEVAL_MAX_CHARS_PER_CHUNK: usize = 400;
pub const RETRIEVAL_TOTAL_CHAR_BUDGET: usize = 2_400;
pub const RETRIEVAL_MAX_CHUNKS_PER_MEMORY: usize = 2;
pub const RETRIEVAL_TOTAL_CHAR_BUDGET_PER_MEMORY: usize = 800;

/// Apply a penalty to memory quality score when content is oversized.
///
/// Policy:
/// - <= 2,500 chars: no penalty
/// - 2,501..=8,000 chars: linear penalty up to -0.15
/// - > 8,000 chars: capped penalty of -0.20
pub fn large_memory_quality_penalty(content_len: usize) -> f32 {
    if content_len <= MEMORY_WARNING_LENGTH {
        0.0
    } else if content_len <= 8_000 {
        let excess = (content_len - MEMORY_WARNING_LENGTH) as f32;
        let span = (8_000 - MEMORY_WARNING_LENGTH) as f32;
        0.15 * (excess / span)
    } else {
        0.20
    }
}
