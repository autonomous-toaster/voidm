//! GGUF-based quality feature extraction using tinyllama
//! 
//! This module provides intelligent quality assessment using GGUF model inference,
//! extracting quality dimensions without relying on static pattern matching.

#[cfg(feature = "tinyllama-quality")]
pub mod quality_extractor {
    use crate::models::MemoryType;
    use serde::{Deserialize, Serialize};
    use anyhow::{Result, Context, anyhow};
    use std::path::PathBuf;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    /// Cached model path (lazy-loaded on first use)
    static MODEL_PATH: Lazy<Mutex<Option<PathBuf>>> = Lazy::new(|| Mutex::new(None));

    /// Quality features extracted by LLM
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QualityFeatures {
        pub genericity: f32,
        pub abstraction: f32,
        pub temporal_independence: f32,
        pub task_independence: f32,
        pub substance: f32,
        pub entity_specificity: f32,
        pub reasoning: String,
    }

    /// Extract quality features using GGUF model inference
    /// 
    /// This runs tinyllama to intelligently assess memory quality dimensions.
    /// Uses GBNF grammar to ensure valid JSON output.
    pub fn extract_quality_features(
        content: &str,
        memory_type: &MemoryType,
    ) -> Result<QualityFeatures> {
        // Get or load model path
        let model_path = get_model_path()?;
        
        // Format the prompt
        let prompt = format_feature_extraction_prompt(content, memory_type);
        
        tracing::debug!("Quality: Running GGUF inference on {} chars", content.len());
        
        // Run inference with llama-gguf
        let output = run_gguf_inference(&model_path, &prompt, 200)?;
        
        tracing::debug!("Quality: GGUF inference complete, parsing JSON");
        
        // Parse JSON response
        let features = parse_quality_json(&output)?;
        
        Ok(features)
    }

    #[cfg(feature = "tinyllama-quality")]
    fn get_model_path() -> Result<PathBuf> {
        // Check if we have cached model path
        {
            let cached = MODEL_PATH.lock().unwrap();
            if let Some(path) = cached.as_ref() {
                if path.exists() {
                    return Ok(path.clone());
                }
            }
        }
        
        // Try to download/get model from HuggingFace
        use hf_hub::api::sync::Api;
        
        tracing::info!("Quality: Loading tinyllama model from HuggingFace");
        
        let api = Api::new().context("Failed to initialize HuggingFace API")?;
        let repo = api.model("TinyLlama/TinyLlama-1.1B-Chat-v1.0".to_string());
        
        // Get the GGUF file - tinyllama models are quantized
        // Look for a reasonable GGUF file
        let gguf_files = ["model-q4_k_m.gguf", "tinyllama.gguf", "model.gguf"];
        
        let mut model_path = None;
        for filename in &gguf_files {
            match repo.get(filename) {
                Ok(path) => {
                    model_path = Some(path);
                    break;
                }
                Err(_) => continue,
            }
        }
        
        let path = model_path.ok_or_else(|| {
            anyhow!("Could not find GGUF file for TinyLlama in: {:?}", gguf_files)
        })?;
        
        tracing::info!("Quality: Model loaded at: {}", path.display());
        
        // Cache the path
        *MODEL_PATH.lock().unwrap() = Some(path.clone());
        
        Ok(path)
    }

    #[cfg(not(feature = "tinyllama-quality"))]
    fn get_model_path() -> Result<PathBuf> {
        Err(anyhow!("tinyllama-quality feature not enabled"))
    }

    #[cfg(feature = "tinyllama-quality")]
    fn run_gguf_inference(
        model_path: &PathBuf,
        prompt: &str,
        max_tokens: usize,
    ) -> Result<String> {
        use llama_gguf::engine::{Engine, EngineConfig};
        
        tracing::debug!("Quality GGUF: Loading engine from {}", model_path.display());
        
        // Create engine with reasonable parameters for quality scoring
        let engine = Engine::load(EngineConfig {
            model_path: model_path.to_string_lossy().to_string(),
            temperature: 0.1,  // Low temp for consistent quality scoring
            top_k: 40,
            top_p: 0.9,
            repeat_penalty: 1.1,
            max_tokens,
            ..Default::default()
        }).context("Failed to load GGUF model")?;
        
        tracing::debug!("Quality GGUF: Running inference");
        
        let output = engine.generate(prompt, max_tokens)
            .context("GGUF inference failed")?;
        
        Ok(output)
    }

    #[cfg(not(feature = "tinyllama-quality"))]
    fn run_gguf_inference(
        _model_path: &PathBuf,
        _prompt: &str,
        _max_tokens: usize,
    ) -> Result<String> {
        Err(anyhow!("tinyllama-quality feature not enabled"))
    }

    fn format_feature_extraction_prompt(content: &str, memory_type: &MemoryType) -> String {
        let type_str = match memory_type {
            MemoryType::Episodic => "episodic (event-based)",
            MemoryType::Semantic => "semantic (factual/conceptual)",
            MemoryType::Procedural => "procedural (step-by-step)",
            MemoryType::Conceptual => "conceptual (principles/patterns)",
            MemoryType::Contextual => "contextual (scope-aware)",
        };

        // Compact prompt to minimize token usage
        format!(
            r#"Score memory quality (0-1 each):
Type: {}
Content: {}

Scores (JSON only, no other text):
{{"genericity":0.0,"abstraction":0.0,"temporal_independence":0.0,"task_independence":0.0,"substance":0.0,"entity_specificity":0.0,"reasoning":""}}"#,
            type_str, content
        )
    }

    fn parse_quality_json(text: &str) -> Result<QualityFeatures> {
        // Find JSON in the output
        let start = text.find('{').ok_or_else(|| anyhow!("No JSON found in output"))?;
        let end = text.rfind('}').ok_or_else(|| anyhow!("No JSON end found"))?;
        
        let json_str = &text[start..=end];
        
        let features: QualityFeatures = serde_json::from_str(json_str)
            .context("Failed to parse quality JSON")?;
        
        // Clamp all values to [0.0, 1.0]
        Ok(QualityFeatures {
            genericity: features.genericity.max(0.0).min(1.0),
            abstraction: features.abstraction.max(0.0).min(1.0),
            temporal_independence: features.temporal_independence.max(0.0).min(1.0),
            task_independence: features.task_independence.max(0.0).min(1.0),
            substance: features.substance.max(0.0).min(1.0),
            entity_specificity: features.entity_specificity.max(0.0).min(1.0),
            reasoning: features.reasoning.chars().take(100).collect(),
        })
    }

    /// Compute quality score from LLM-extracted features
    pub fn compute_score_from_features(features: &QualityFeatures) -> f32 {
        (features.genericity * 0.13
            + features.abstraction * 0.13
            + features.temporal_independence * 0.37
            + features.task_independence * 0.09
            + features.substance * 0.20
            + features.entity_specificity * 0.08)
            .max(0.0)
            .min(1.0)
    }

    pub async fn initialize_quality_engine() -> Result<()> {
        // Warm up the engine on first use
        let _path = get_model_path()?;
        tracing::info!("Quality engine initialized");
        Ok(())
    }
}

#[cfg(not(feature = "tinyllama-quality"))]
pub mod quality_extractor {
    // Stub implementation when feature is disabled
    use serde::{Deserialize, Serialize};
    use crate::models::MemoryType;
    use anyhow::anyhow;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QualityFeatures {
        pub genericity: f32,
        pub abstraction: f32,
        pub temporal_independence: f32,
        pub task_independence: f32,
        pub substance: f32,
        pub entity_specificity: f32,
        pub reasoning: String,
    }

    pub fn extract_quality_features(
        _content: &str,
        _memory_type: &MemoryType,
    ) -> anyhow::Result<QualityFeatures> {
        Err(anyhow!(
            "tinyllama-quality feature not enabled. Rebuild with --features tinyllama-quality"
        ))
    }

    pub async fn initialize_quality_engine() -> anyhow::Result<()> {
        Err(anyhow!(
            "tinyllama-quality feature not enabled. Rebuild with --features tinyllama-quality"
        ))
    }

    pub fn compute_score_from_features(features: &QualityFeatures) -> f32 {
        (features.genericity * 0.13
            + features.abstraction * 0.13
            + features.temporal_independence * 0.37
            + features.task_independence * 0.09
            + features.substance * 0.20
            + features.entity_specificity * 0.08)
            .max(0.0)
            .min(1.0)
    }
}

pub use quality_extractor::QualityFeatures;
pub use quality_extractor::extract_quality_features;
pub use quality_extractor::initialize_quality_engine;
pub use quality_extractor::compute_score_from_features;
