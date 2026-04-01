//! JSONL Export for voidm memories, chunks, and relationships
//!
//! Enables portable, human-readable backup and migration between backends.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;

/// Export record type (discriminated union)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ExportRecord {
    #[serde(rename = "memory")]
    Memory(MemoryRecord),
    #[serde(rename = "memory_chunk")]
    MemoryChunk(ChunkRecord),
    #[serde(rename = "relationship")]
    Relationship(RelationshipRecord),
    #[serde(rename = "concept")]
    Concept(ConceptRecord),
}

/// Memory export record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    #[serde(rename = "memory_type")]
    pub memory_type: String,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub title: Option<String>,
    pub scope: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    pub provenance: Option<String>,
    pub context: Option<String>,
    pub importance: Option<u8>,
    pub quality_score: Option<f32>,
}

/// Memory chunk export record (with embeddings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub id: String,
    pub memory_id: String,
    pub content: String,
    pub created_at: String,
    pub coherence_score: Option<f32>,
    pub quality: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub embedding_dim: Option<usize>,
    pub embedding_model: Option<String>,
}

/// Relationship export record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipRecord {
    pub source_id: String,
    pub rel_type: String,
    pub target_id: String,
    pub note: Option<String>,
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,
}

/// Concept export record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<String>,
    pub created_at: Option<String>,
}

/// Export statistics
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExportStats {
    pub total_memories: usize,
    pub total_chunks: usize,
    pub total_relationships: usize,
    pub total_concepts: usize,
    pub total_records: usize,
    pub file_size_bytes: usize,
    pub duration_ms: u128,
}

impl ExportStats {
    pub fn total_records(&self) -> usize {
        self.total_memories + self.total_chunks + self.total_relationships + self.total_concepts
    }
}

/// Serialize a single record to JSONL format
pub fn record_to_jsonl(record: &ExportRecord) -> Result<String> {
    serde_json::to_string(record)
        .map_err(|e| anyhow!("Failed to serialize record: {}", e))
}

/// Deserialize a single JSONL line to a record
pub fn jsonl_to_record(line: &str) -> Result<ExportRecord> {
    serde_json::from_str(line)
        .map_err(|e| anyhow!("Failed to deserialize record: {}", e))
}

/// Write record to file (with newline)
pub fn write_record_to_file<W: Write>(writer: &mut W, record: &ExportRecord) -> Result<()> {
    let json = record_to_jsonl(record)?;
    writeln!(writer, "{}", json)
        .map_err(|e| anyhow!("Failed to write record: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_serialization() {
        let record = ExportRecord::Memory(MemoryRecord {
            id: "test-id".to_string(),
            content: "test content".to_string(),
            memory_type: "semantic".to_string(),
            created_at: "2026-03-28T00:00:00Z".to_string(),
            updated_at: None,
            title: Some("Test Title".to_string()),
            scope: Some("test".to_string()),
            scopes: Some(vec!["test".to_string()]),
            tags: Some(vec!["tag1".to_string()]),
            metadata: Some(serde_json::json!({"key": "value"})),
            provenance: Some("user".to_string()),
            context: Some("decision".to_string()),
            importance: Some(5),
            quality_score: Some(0.8),
        });

        let json = record_to_jsonl(&record).unwrap();
        assert!(json.contains("\"type\":\"memory\""));
        assert!(json.contains("\"id\":\"test-id\""));
        assert!(json.contains("\"content\":\"test content\""));
        assert!(json.contains("\"title\":\"Test Title\""));

        let parsed = jsonl_to_record(&json).unwrap();
        match parsed {
            ExportRecord::Memory(m) => {
                assert_eq!(m.id, "test-id");
                assert_eq!(m.content, "test content");
                assert_eq!(m.title, Some("Test Title".to_string()));
                assert!(m.metadata.is_some());
            }
            _ => panic!("Expected Memory record"),
        }
    }

    #[test]
    fn test_chunk_with_embedding() {
        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        let record = ExportRecord::MemoryChunk(ChunkRecord {
            id: "chunk-id".to_string(),
            memory_id: "mem-id".to_string(),
            content: "chunk text".to_string(),
            created_at: "2026-03-28T00:00:00Z".to_string(),
            coherence_score: Some(0.75),
            quality: Some("GOOD".to_string()),
            embedding: Some(embedding.clone()),
            embedding_dim: Some(4),
            embedding_model: Some("test-model".to_string()),
        });

        let json = record_to_jsonl(&record).unwrap();
        let parsed = jsonl_to_record(&json).unwrap();

        match parsed {
            ExportRecord::MemoryChunk(c) => {
                assert_eq!(c.id, "chunk-id");
                assert_eq!(c.embedding, Some(embedding));
                assert_eq!(c.embedding_dim, Some(4));
            }
            _ => panic!("Expected MemoryChunk record"),
        }
    }

    #[test]
    fn test_relationship_serialization() {
        let record = ExportRecord::Relationship(RelationshipRecord {
            source_id: "src".to_string(),
            rel_type: "SUPPORTS".to_string(),
            target_id: "tgt".to_string(),
            note: Some("supports this".to_string()),
            created_at: Some("2026-03-28T00:00:00Z".to_string()),
            properties: None,
        });

        let json = record_to_jsonl(&record).unwrap();
        let parsed = jsonl_to_record(&json).unwrap();

        match parsed {
            ExportRecord::Relationship(r) => {
                assert_eq!(r.source_id, "src");
                assert_eq!(r.rel_type, "SUPPORTS");
                assert_eq!(r.target_id, "tgt");
            }
            _ => panic!("Expected Relationship record"),
        }
    }

    #[test]
    fn test_concept_serialization() {
        let record = ExportRecord::Concept(ConceptRecord {
            id: "concept-id".to_string(),
            name: "Test Concept".to_string(),
            description: Some("A test concept".to_string()),
            scope: Some("test".to_string()),
            created_at: Some("2026-03-28T00:00:00Z".to_string()),
        });

        let json = record_to_jsonl(&record).unwrap();
        let parsed = jsonl_to_record(&json).unwrap();

        match parsed {
            ExportRecord::Concept(c) => {
                assert_eq!(c.name, "Test Concept");
                assert_eq!(c.id, "concept-id");
            }
            _ => panic!("Expected Concept record"),
        }
    }

    #[test]
    fn test_memory_record_type_roundtrip() {
        let record = ExportRecord::Memory(MemoryRecord {
            id: "mem-type-test".to_string(),
            content: "typed memory".to_string(),
            memory_type: "procedural".to_string(),
            created_at: "2026-03-31T00:00:00Z".to_string(),
            updated_at: Some("2026-03-31T00:00:00Z".to_string()),
            title: None,
            scope: None,
            scopes: Some(vec!["work".to_string()]),
            tags: Some(vec!["ops".to_string()]),
            metadata: Some(serde_json::json!({"a": 1})),
            provenance: None,
            context: None,
            importance: Some(5),
            quality_score: Some(0.7),
        });

        let json = record_to_jsonl(&record).unwrap();
        let parsed = jsonl_to_record(&json).unwrap();
        match parsed {
            ExportRecord::Memory(m) => assert_eq!(m.memory_type, "procedural"),
            _ => panic!("Expected memory record"),
        }
    }

    #[test]
    fn test_embedding_roundtrip() {
        let embedding: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();
        let chunk = ChunkRecord {
            id: "test".to_string(),
            memory_id: "mem".to_string(),
            content: "test".to_string(),
            created_at: "2026-03-28T00:00:00Z".to_string(),
            coherence_score: None,
            quality: None,
            embedding: Some(embedding.clone()),
            embedding_dim: Some(384),
            embedding_model: Some("all-MiniLM-L6-v2".to_string()),
        };

        let record = ExportRecord::MemoryChunk(chunk);
        let json = record_to_jsonl(&record).unwrap();
        let parsed = jsonl_to_record(&json).unwrap();

        match parsed {
            ExportRecord::MemoryChunk(c) => {
                assert_eq!(c.embedding_dim, Some(384));
                if let Some(emb) = c.embedding {
                    assert_eq!(emb.len(), 384);
                    assert!((emb[0] - 0.0).abs() < 0.01);
                    assert!((emb[383] - 0.999).abs() < 0.01);
                }
            }
            _ => panic!("Expected MemoryChunk"),
        }
    }
}
