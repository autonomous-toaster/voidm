#[cfg(test)]
mod tests {
    use super::super::*;
    use std::sync::Arc;

    /// Test that the Database trait can be used as a trait object
    #[test]
    fn test_database_trait_is_object_safe() {
        // This compiles if the trait is object-safe
        let _: Arc<dyn Database> = Arc::new(DummyDatabase);
    }

    /// Dummy implementation for testing
    struct DummyDatabase;

    impl Database for DummyDatabase {
        fn health_check(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn close(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn ensure_schema(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn add_memory(
            &self,
            _req: crate::models::AddMemoryRequest,
            _config: &crate::Config,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<crate::models::AddMemoryResponse>> + Send + '_>> {
            Box::pin(async {
                anyhow::bail!("dummy")
            })
        }

        fn get_memory(&self, _id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<crate::models::Memory>>> + Send + '_>> {
            Box::pin(async { Ok(None) })
        }

        fn list_memories(&self, _limit: Option<usize>) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<crate::models::Memory>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn delete_memory(&self, _id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(false) })
        }

        fn update_memory(
            &self,
            _id: &str,
            _content: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }

        fn resolve_memory_id(&self, _id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
            Box::pin(async { Ok(String::new()) })
        }

        fn list_scopes(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<String>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn link_memories(
            &self,
            _from_id: &str,
            _rel: &crate::models::EdgeType,
            _to_id: &str,
            _note: Option<&str>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<crate::models::Edge>> + Send + '_>> {
            Box::pin(async { anyhow::bail!("dummy") })
        }

        fn unlink_memories(
            &self,
            _from_id: &str,
            _rel: &crate::models::EdgeType,
            _to_id: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(false) })
        }

        fn search_hybrid(
            &self,
            _opts: &crate::models::SearchOptions,
            _model_name: &str,
            _embeddings_enabled: bool,
            _config_min_score: f32,
            _config_search: &crate::config::SearchConfig,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<crate::models::SearchResponse>> + Send + '_>> {
            Box::pin(async { anyhow::bail!("dummy") })
        }

        fn add_concept(
            &self,
            _name: &str,
            _description: Option<&str>,
            _scope: Option<&str>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<crate::models::Concept>> + Send + '_>> {
            Box::pin(async { anyhow::bail!("dummy") })
        }

        fn get_concept(&self, _id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<crate::models::Concept>> + Send + '_>> {
            Box::pin(async { anyhow::bail!("dummy") })
        }

        fn get_concept_with_instances(
            &self,
            _id: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<crate::models::ConceptWithInstances>> + Send + '_>> {
            Box::pin(async { anyhow::bail!("dummy") })
        }

        fn list_concepts(
            &self,
            _scope: Option<&str>,
            _limit: usize,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<crate::models::Concept>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn delete_concept(&self, _id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(false) })
        }

        fn resolve_concept_id(&self, _id: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
            Box::pin(async { Ok(String::new()) })
        }

        fn search_concepts(
            &self,
            _query: &str,
            _scope: Option<&str>,
            _limit: usize,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<crate::models::Concept>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn add_ontology_edge(
            &self,
            _from_id: &str,
            _from_kind: crate::ontology::NodeKind,
            _rel: &crate::ontology::OntologyRelType,
            _to_id: &str,
            _to_kind: crate::ontology::NodeKind,
            _note: Option<&str>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<crate::ontology::ConceptEdge>> + Send + '_>> {
            Box::pin(async { anyhow::bail!("dummy") })
        }

        fn delete_ontology_edge(&self, _edge_id: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>> + Send + '_>> {
            Box::pin(async { Ok(false) })
        }

        fn query_cypher(
            &self,
            _query: &str,
            _params: &serde_json::Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<serde_json::Value>> + Send + '_>> {
            Box::pin(async { anyhow::bail!("dummy") })
        }

        fn get_neighbors(
            &self,
            _id: &str,
            _depth: usize,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<serde_json::Value>> + Send + '_>> {
            Box::pin(async { anyhow::bail!("dummy") })
        }

        fn check_model_mismatch(
            &self,
            _configured_model: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<(String, String)>>> + Send + '_>> {
            Box::pin(async { Ok(None) })
        }
    }
}
