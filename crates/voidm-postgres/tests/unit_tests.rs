use serde_json::json;
use voidm_postgres::MemoryQueryBuilder;

#[test]
fn test_query_builder_single_filter() {
    let query = MemoryQueryBuilder::new()
        .memory_type("semantic")
        .build();
    
    assert!(query.contains("memory_type = 'semantic'"));
    assert!(query.contains("ORDER BY created_at DESC"));
}

#[test]
fn test_query_builder_multiple_filters() {
    let query = MemoryQueryBuilder::new()
        .memory_type("episodic")
        .has_scope("dev")
        .min_importance(5)
        .build();
    
    assert!(query.contains("memory_type = 'episodic'"));
    assert!(query.contains("scopes @>"));
    assert!(query.contains("importance >= 5"));
    assert!(query.contains("AND"));
}

#[test]
fn test_query_builder_empty() {
    let query = MemoryQueryBuilder::new().build();
    
    assert!(query.contains("SELECT id::text, content, memory_type FROM memories"));
    assert!(query.contains("ORDER BY created_at DESC"));
    assert!(!query.contains("WHERE"));
}

#[test]
fn test_query_builder_content_contains() {
    let query = MemoryQueryBuilder::new()
        .content_contains("rust")
        .build();
    
    assert!(query.contains("content ILIKE '%rust%'"));
}

#[test]
fn test_validate_memory_request_valid() {
    let req = json!({
        "content": "Test memory",
        "memory_type": "semantic",
        "tags": ["test"],
        "scopes": ["dev"]
    });
    
    assert!(voidm_postgres::validate_memory_request(&req).is_ok());
}

#[test]
fn test_validate_memory_request_missing_content() {
    let req = json!({
        "memory_type": "semantic"
    });
    
    assert!(voidm_postgres::validate_memory_request(&req).is_err());
}

#[test]
fn test_validate_memory_request_invalid_tags() {
    let req = json!({
        "content": "Test",
        "tags": "not_an_array"
    });
    
    assert!(voidm_postgres::validate_memory_request(&req).is_err());
}

#[test]
fn test_validate_concept_name_valid() {
    assert!(voidm_postgres::validate_concept_request("ValidConcept").is_ok());
}

#[test]
fn test_validate_concept_name_empty() {
    assert!(voidm_postgres::validate_concept_request("").is_err());
}

#[test]
fn test_validate_concept_name_too_long() {
    let long_name = "a".repeat(300);
    assert!(voidm_postgres::validate_concept_request(&long_name).is_err());
}
