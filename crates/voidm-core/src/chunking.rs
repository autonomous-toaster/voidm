pub use voidm_embeddings::{BreakType, ChunkingConfig as ChunkingStrategy, OwnedChunk as Chunk};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_types_exported() {
        let strategy = ChunkingStrategy::default();
        assert!(strategy.smart_breaks);
    }

    #[test]
    fn test_break_type_available() {
        let kind = BreakType::Paragraph;
        assert!(matches!(kind, BreakType::Paragraph));
    }
}
