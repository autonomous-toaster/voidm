//! Backend-agnostic CRUD preparation logic
//!
//! This module contains the shared business logic for memory creation that is
//! independent of the backend (SQLite, Neo4j, PostgreSQL). It handles:
//!
//! - ID generation
//! - Quality score computation
//! - Timestamp generation
//! - Metadata extraction and validation
//! - All pre-computation needed before backend execution
//!
//! Backends should use this module to prepare requests, then execute their
//! backend-specific queries (SQL, Cypher, etc.) with the prepared values.

use crate::models::{AddMemoryRequest, MemoryType};
use anyhow::Result;
use serde_json::Value;
use uuid::Uuid;

/// Pre-computed memory creation data ready for backend execution
///
/// All business logic has been applied; backends just need to execute
/// their database queries with these values.
#[derive(Debug, Clone)]
pub struct PreparedMemory {
    pub id: String,
    pub content: String,
    pub memory_type: MemoryType,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub importance: i64,
    pub metadata: Value,
    pub context: Option<String>,
    pub title: Option<String>,
    pub created_at: String,
    pub quality_score: f32,
    pub author: String,
    pub source: String,
}

/// Prepares memory creation request with all business logic applied
pub struct MemoryCreationPreparer {
    req: AddMemoryRequest,
}

impl MemoryCreationPreparer {
    /// Create new preparer from request
    pub fn new(req: AddMemoryRequest) -> Self {
        Self { req }
    }

    /// Prepare the request by applying all business logic
    pub fn prepare(self) -> Result<PreparedMemory> {
        // Generate ID if not provided
        let id = self.req.id.unwrap_or_else(|| Uuid::new_v4().to_string());

        // Generate timestamp
        let created_at = chrono::Utc::now().to_rfc3339();

        // Compute quality score
        let quality_score = compute_quality_score(&self.req.content, &self.req.memory_type);

        // Extract author and source from metadata
        let author = extract_metadata_field(&self.req.metadata, "author")
            .unwrap_or_else(|| "user".to_string());

        let source = extract_metadata_field(&self.req.metadata, "source_reliability")
            .or_else(|| extract_metadata_field(&self.req.metadata, "source"))
            .unwrap_or_else(|| "unknown".to_string());

        Ok(PreparedMemory {
            id,
            content: self.req.content,
            memory_type: self.req.memory_type,
            scopes: self.req.scopes,
            tags: self.req.tags,
            importance: self.req.importance,
            metadata: self.req.metadata,
            context: self.req.context,
            title: self.req.title,
            created_at,
            quality_score,
            author,
            source,
        })
    }
}

/// Compute quality score for a memory
fn compute_quality_score(content: &str, memory_type: &MemoryType) -> f32 {
    // Convert memory type to voidm_scoring type
    let quality_mt = match memory_type {
        MemoryType::Episodic => voidm_scoring::MemoryType::Episodic,
        MemoryType::Semantic => voidm_scoring::MemoryType::Semantic,
        MemoryType::Procedural => voidm_scoring::MemoryType::Procedural,
        MemoryType::Conceptual => voidm_scoring::MemoryType::Conceptual,
        MemoryType::Contextual => voidm_scoring::MemoryType::Contextual,
    };

    // Use voidm_scoring to compute
    let quality = voidm_scoring::compute_quality_score(content, &quality_mt);
    quality.score
}

/// Extract string field from metadata JSON object
fn extract_metadata_field(metadata: &Value, field: &str) -> Option<String> {
    metadata
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_generation() {
        let req = AddMemoryRequest {
            id: None,
            memory_type: MemoryType::Semantic,
            content: "test".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
            context: None,
            title: None,
        };

        let prepared = MemoryCreationPreparer::new(req).prepare().unwrap();
        assert!(!prepared.id.is_empty());
        assert!(prepared.id.len() == 36); // UUID v4 length
    }

    #[test]
    fn test_id_preservation() {
        let req = AddMemoryRequest {
            id: Some("custom-id".to_string()),
            memory_type: MemoryType::Semantic,
            content: "test".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
            context: None,
            title: None,
        };

        let prepared = MemoryCreationPreparer::new(req).prepare().unwrap();
        assert_eq!(prepared.id, "custom-id");
    }

    #[test]
    fn test_author_extraction() {
        let req = AddMemoryRequest {
            id: None,
            memory_type: MemoryType::Semantic,
            content: "test".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({ "author": "assistant" }),
            links: vec![],
            context: None,
            title: None,
        };

        let prepared = MemoryCreationPreparer::new(req).prepare().unwrap();
        assert_eq!(prepared.author, "assistant");
    }

    #[test]
    fn test_author_default() {
        let req = AddMemoryRequest {
            id: None,
            memory_type: MemoryType::Semantic,
            content: "test".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
            context: None,
            title: None,
        };

        let prepared = MemoryCreationPreparer::new(req).prepare().unwrap();
        assert_eq!(prepared.author, "user");
    }

    #[test]
    fn test_source_extraction() {
        let req = AddMemoryRequest {
            id: None,
            memory_type: MemoryType::Semantic,
            content: "test".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({ "source_reliability": "verified" }),
            links: vec![],
            context: None,
            title: None,
        };

        let prepared = MemoryCreationPreparer::new(req).prepare().unwrap();
        assert_eq!(prepared.source, "verified");
    }

    #[test]
    fn test_fields_preserved() {
        let req = AddMemoryRequest {
            id: None,
            memory_type: MemoryType::Episodic,
            content: "test content".to_string(),
            scopes: vec!["scope1".to_string(), "scope2".to_string()],
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            importance: 8,
            metadata: serde_json::json!({ "key": "value" }),
            links: vec![],
            context: Some("decision".to_string()),
            title: Some("Test Title".to_string()),
        };

        let prepared = MemoryCreationPreparer::new(req).prepare().unwrap();
        assert_eq!(prepared.content, "test content");
        assert_eq!(prepared.memory_type, MemoryType::Episodic);
        assert_eq!(prepared.scopes, vec!["scope1", "scope2"]);
        assert_eq!(prepared.tags, vec!["tag1", "tag2"]);
        assert_eq!(prepared.importance, 8);
        assert_eq!(prepared.context, Some("decision".to_string()));
        assert_eq!(prepared.title, Some("Test Title".to_string()));
    }

    #[test]
    fn test_quality_score_computed() {
        let req = AddMemoryRequest {
            id: None,
            memory_type: MemoryType::Semantic,
            content: "This is a substantial memory with meaningful content that should score well".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
            context: None,
            title: None,
        };

        let prepared = MemoryCreationPreparer::new(req).prepare().unwrap();
        assert!(prepared.quality_score > 0.0);
        assert!(prepared.quality_score <= 1.0);
    }

    #[test]
    fn test_timestamp_generated() {
        let req = AddMemoryRequest {
            id: None,
            memory_type: MemoryType::Semantic,
            content: "test".to_string(),
            scopes: vec![],
            tags: vec![],
            importance: 5,
            metadata: serde_json::json!({}),
            links: vec![],
            context: None,
            title: None,
        };

        let prepared = MemoryCreationPreparer::new(req).prepare().unwrap();
        assert!(!prepared.created_at.is_empty());
        // Should be valid RFC3339 timestamp
        assert!(prepared.created_at.contains('T'));
        assert!(prepared.created_at.contains('Z') || prepared.created_at.contains('+'));
    }
}
