//! JSONL Import for voidm memories, chunks, and relationships
//!
//! Validates and imports records from portable JSONL format.

use anyhow::{anyhow, Result};
use std::io::{BufRead, BufReader, Read};
use std::collections::HashSet;

use crate::export::{ExportRecord, MemoryRecord, ChunkRecord, RelationshipRecord, ConceptRecord};

/// Import statistics
#[derive(Debug, Clone, Default)]
pub struct ImportStats {
    pub total_records_read: usize,
    pub memories_imported: usize,
    pub chunks_imported: usize,
    pub relationships_imported: usize,
    pub concepts_imported: usize,
    pub records_skipped: usize,
    pub records_with_errors: usize,
    pub duration_ms: u128,
}

impl ImportStats {
    pub fn total_imported(&self) -> usize {
        self.memories_imported + self.chunks_imported + self.relationships_imported + self.concepts_imported
    }
}

/// Validation result for a record
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    pub fn invalid(error: impl Into<String>) -> Self {
        Self {
            is_valid: false,
            errors: vec![error.into()],
            warnings: vec![],
        }
    }

    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

/// Validate a memory record
pub fn validate_memory(record: &MemoryRecord) -> ValidationResult {
    let mut result = ValidationResult::valid();

    if record.id.is_empty() {
        result.is_valid = false;
        result.errors.push("Memory ID cannot be empty".to_string());
    }

    if record.content.is_empty() {
        result.is_valid = false;
        result.errors.push("Memory content cannot be empty".to_string());
    }

    // Validate memory type
    match record.memory_type.as_str() {
        "episodic" | "semantic" | "procedural" | "conceptual" | "contextual" => {}
        _ => {
            result.is_valid = false;
            result.errors.push(format!(
                "Invalid memory type: {}. Must be one of: episodic, semantic, procedural, conceptual, contextual",
                record.memory_type
            ));
        }
    }

    // Validate importance if present
    if let Some(imp) = record.importance {
        if imp > 10 {
            result = result.with_warning(format!("Importance {} exceeds max of 10, clamping", imp));
        }
    }

    // Validate quality score if present
    if let Some(score) = record.quality_score {
        if !(0.0..=1.0).contains(&score) {
            result = result.with_warning(format!(
                "Quality score {} outside range [0.0, 1.0], clamping",
                score
            ));
        }
    }

    result
}

/// Validate a chunk record
pub fn validate_chunk(record: &ChunkRecord) -> ValidationResult {
    let mut result = ValidationResult::valid();

    if record.id.is_empty() {
        result.is_valid = false;
        result.errors.push("Chunk ID cannot be empty".to_string());
    }

    if record.memory_id.is_empty() {
        result.is_valid = false;
        result.errors.push("Chunk memory_id cannot be empty".to_string());
    }

    if record.content.is_empty() {
        result.is_valid = false;
        result.errors.push("Chunk content cannot be empty".to_string());
    }

    // Validate embedding consistency
    if let Some(ref embedding) = record.embedding {
        if let Some(dim) = record.embedding_dim {
            if embedding.len() != dim {
                result.is_valid = false;
                result.errors.push(format!(
                    "Embedding length {} does not match embedding_dim {}",
                    embedding.len(),
                    dim
                ));
            }
        } else {
            result = result.with_warning("Embedding present but embedding_dim is missing".to_string());
        }
    }

    // Validate quality if present
    if let Some(ref quality) = record.quality {
        match quality.as_str() {
            "GOOD" | "FAIR" | "EXCELLENT" => {}
            _ => {
                result = result.with_warning(format!(
                    "Unknown quality value: {}. Should be GOOD, FAIR, or EXCELLENT",
                    quality
                ));
            }
        }
    }

    // Validate coherence score if present
    if let Some(score) = record.coherence_score {
        if !(0.0..=1.0).contains(&score) {
            result = result.with_warning(format!(
                "Coherence score {} outside range [0.0, 1.0]",
                score
            ));
        }
    }

    result
}

/// Validate a relationship record
pub fn validate_relationship(record: &RelationshipRecord) -> ValidationResult {
    let mut result = ValidationResult::valid();

    if record.source_id.is_empty() {
        result.is_valid = false;
        result.errors.push("Relationship source_id cannot be empty".to_string());
    }

    if record.target_id.is_empty() {
        result.is_valid = false;
        result.errors.push("Relationship target_id cannot be empty".to_string());
    }

    // Validate rel_type
    match record.rel_type.as_str() {
        "SUPPORTS" | "CONTRADICTS" | "DERIVED_FROM" | "PRECEDES" | "PART_OF" | "EXEMPLIFIES" | "RELATES_TO" => {}
        _ => {
            result.is_valid = false;
            result.errors.push(format!(
                "Invalid relationship type: {}",
                record.rel_type
            ));
        }
    }

    result
}

/// Validate a concept record
pub fn validate_concept(record: &ConceptRecord) -> ValidationResult {
    let mut result = ValidationResult::valid();

    if record.id.is_empty() {
        result.is_valid = false;
        result.errors.push("Concept ID cannot be empty".to_string());
    }

    if record.name.is_empty() {
        result.is_valid = false;
        result.errors.push("Concept name cannot be empty".to_string());
    }

    result
}

/// Validate a record (dispatch)
pub fn validate_record(record: &ExportRecord) -> ValidationResult {
    match record {
        ExportRecord::Memory(r) => validate_memory(r),
        ExportRecord::MemoryChunk(r) => validate_chunk(r),
        ExportRecord::Relationship(r) => validate_relationship(r),
        ExportRecord::Concept(r) => validate_concept(r),
    }
}

/// Read and validate records from JSONL stream
pub fn read_jsonl_records<R: Read>(reader: R) -> Result<Vec<(ExportRecord, ValidationResult)>> {
    let buf_reader = BufReader::new(reader);
    let mut records = Vec::new();

    for (line_num, line_result) in buf_reader.lines().enumerate() {
        let line = line_result.map_err(|e| anyhow!("Failed to read line {}: {}", line_num + 1, e))?;

        if line.trim().is_empty() {
            continue; // Skip empty lines
        }

        match serde_json::from_str::<ExportRecord>(&line) {
            Ok(record) => {
                let validation = validate_record(&record);
                records.push((record, validation));
            }
            Err(e) => {
                return Err(anyhow!(
                    "Failed to parse JSONL at line {}: {}",
                    line_num + 1,
                    e
                ));
            }
        }
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_memory_valid() {
        let mem = MemoryRecord {
            id: "test-id".to_string(),
            content: "content".to_string(),
            memory_type: "semantic".to_string(),
            created_at: "2026-03-28T00:00:00Z".to_string(),
            updated_at: None,
            scope: None,
            tags: None,
            provenance: None,
            context: None,
            importance: Some(5),
            quality_score: Some(0.8),
        };

        let result = validate_memory(&mem);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_memory_invalid_type() {
        let mem = MemoryRecord {
            id: "test".to_string(),
            content: "content".to_string(),
            memory_type: "invalid".to_string(),
            created_at: "2026-03-28T00:00:00Z".to_string(),
            updated_at: None,
            scope: None,
            tags: None,
            provenance: None,
            context: None,
            importance: None,
            quality_score: None,
        };

        let result = validate_memory(&mem);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_chunk_with_embedding() {
        let chunk = ChunkRecord {
            id: "chunk-id".to_string(),
            memory_id: "mem-id".to_string(),
            content: "content".to_string(),
            created_at: "2026-03-28T00:00:00Z".to_string(),
            coherence_score: Some(0.75),
            quality: Some("GOOD".to_string()),
            embedding: Some(vec![0.1, 0.2, 0.3]),
            embedding_dim: Some(3),
            embedding_model: Some("model".to_string()),
        };

        let result = validate_chunk(&chunk);
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_chunk_embedding_mismatch() {
        let chunk = ChunkRecord {
            id: "chunk-id".to_string(),
            memory_id: "mem-id".to_string(),
            content: "content".to_string(),
            created_at: "2026-03-28T00:00:00Z".to_string(),
            coherence_score: None,
            quality: None,
            embedding: Some(vec![0.1, 0.2, 0.3]),
            embedding_dim: Some(4), // Mismatch!
            embedding_model: None,
        };

        let result = validate_chunk(&chunk);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_relationship() {
        let rel = RelationshipRecord {
            source_id: "src".to_string(),
            rel_type: "SUPPORTS".to_string(),
            target_id: "tgt".to_string(),
            note: None,
            created_at: None,
        };

        let result = validate_relationship(&rel);
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_relationship_invalid_type() {
        let rel = RelationshipRecord {
            source_id: "src".to_string(),
            rel_type: "INVALID".to_string(),
            target_id: "tgt".to_string(),
            note: None,
            created_at: None,
        };

        let result = validate_relationship(&rel);
        assert!(!result.is_valid);
    }
}
