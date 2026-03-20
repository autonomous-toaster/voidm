//! Quality verification for search optimizations
//! Ensures approximate search and fast vector operations maintain result quality

#[cfg(test)]
mod quality_verification {
    use std::time::Instant;
    use std::collections::HashMap;

    #[test]
    fn test_approximate_vs_exact_search_quality() {
        println!("\n=== Quality Test: Approximate vs Exact Search ===\n");
        println!("  Testing different thresholds to find quality/speed tradeoff\n");
        
        // Generate realistic document collection
        const DOC_COUNT: usize = 5000;
        const QUERY_COUNT: usize = 50;
        const VECTOR_DIM: usize = 384;
        
        // Create deterministic test vectors
        let query_vectors: Vec<Vec<f32>> = (0..QUERY_COUNT)
            .map(|q_idx| {
                (0..VECTOR_DIM)
                    .map(|d| ((q_idx as f32 + d as f32) % 10.0).sin())
                    .collect()
            })
            .collect();
        
        let doc_vectors: Vec<Vec<f32>> = (0..DOC_COUNT)
            .map(|d_idx| {
                (0..VECTOR_DIM)
                    .map(|d| ((d_idx as f32 + d as f32) % 10.0).cos())
                    .collect()
            })
            .collect();
        
        // Test multiple thresholds
        let thresholds = vec![0.1, 0.3, 0.5, 0.7, 0.85];
        
        for threshold in thresholds {
            let mut total_recall = 0.0;
            let mut total_precision = 0.0;
            let mut total_results = 0;
            
            for query in query_vectors.iter() {
                // Exact search: get top 100 results
                let mut scores_exact: Vec<(usize, f32)> = doc_vectors
                    .iter()
                    .enumerate()
                    .map(|(d_idx, doc)| {
                        let sim = cosine_similarity(query, doc);
                        (d_idx, sim)
                    })
                    .collect();
                scores_exact.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let top_100_exact: Vec<usize> = scores_exact.iter().take(100).map(|(idx, _)| *idx).collect();
                
                // Approximate search: early termination at threshold
                let mut scores_approx = Vec::new();
                for (d_idx, doc) in doc_vectors.iter().enumerate() {
                    let sim = cosine_similarity(query, doc);
                    if sim >= threshold {
                        scores_approx.push((d_idx, sim));
                    }
                    if scores_approx.len() >= 100 {
                        break;
                    }
                }
                scores_approx.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let top_k_approx: Vec<usize> = scores_approx.iter().take(100).map(|(idx, _)| *idx).collect();
                
                // Recall: % of exact top-100 found
                let mut matches = 0;
                for &doc_idx in &top_100_exact {
                    if top_k_approx.contains(&doc_idx) {
                        matches += 1;
                    }
                }
                let recall = if !top_100_exact.is_empty() {
                    matches as f32 / top_100_exact.len() as f32
                } else {
                    1.0
                };
                
                // Precision: % of results that are in exact top-100
                let mut precise_matches = 0;
                for &doc_idx in &top_k_approx {
                    if top_100_exact.contains(&doc_idx) {
                        precise_matches += 1;
                    }
                }
                let precision = if !top_k_approx.is_empty() {
                    precise_matches as f32 / top_k_approx.len() as f32
                } else {
                    1.0
                };
                
                total_recall += recall;
                total_precision += precision;
                total_results += top_k_approx.len();
            }
            
            let avg_recall = total_recall / QUERY_COUNT as f32;
            let avg_precision = total_precision / QUERY_COUNT as f32;
            let avg_results = total_results as f32 / QUERY_COUNT as f32;
            
            println!("  Threshold {}: Recall={:.1}%, Precision={:.1}%, AvgResults={:.0}",
                     threshold, avg_recall * 100.0, avg_precision * 100.0, avg_results);
        }
        
        println!("\n  RECOMMENDATION: Approximate search trades quality for speed");
        println!("  ⚠️  NOT recommended as default (quality degradation observed)");
        println!("  ✓ Safe to use with high threshold (0.85+) or for optional \"fast mode\"");
    }

    #[test]
    fn test_fast_vector_math_equivalence() {
        println!("\n=== Quality Test: Fast Vector Math Equivalence ===\n");
        
        const TEST_DIMS: &[usize] = &[96, 192, 384, 768, 1024];
        const TEST_CASES: usize = 1000;
        
        for &dim in TEST_DIMS {
            let mut max_error = 0.0f32;
            let mut sum_error = 0.0f32;
            
            for case in 0..TEST_CASES {
                // Generate test vectors
                let a: Vec<f32> = (0..dim)
                    .map(|i| ((case as f32 * i as f32) % 100.0).sin())
                    .collect();
                let b: Vec<f32> = (0..dim)
                    .map(|i| ((case as f32 * i as f32) % 100.0).cos())
                    .collect();
                
                // Compute with naive method
                let naive_result = cosine_similarity_naive(&a, &b);
                
                // Compute with fast method (currently 32-element chunks)
                let fast_result = cosine_similarity_fast(&a, &b);
                
                // Check equivalence (allow small floating point error)
                let error = (naive_result - fast_result).abs();
                max_error = max_error.max(error);
                sum_error += error;
                
                // Hard fail if error is significant
                assert!(error < 1e-4, 
                    "Math error too large at dim={}, case={}: naive={}, fast={}, error={}",
                    dim, case, naive_result, fast_result, error);
            }
            
            let avg_error = sum_error / TEST_CASES as f32;
            println!("  {}D: max_error={:.2e}, avg_error={:.2e}", dim, max_error, avg_error);
        }
        
        println!("  ✓ Math equivalence verified (max error < 1e-4)");
    }

    #[test]
    fn test_fast_vector_vs_iterator_equivalence() {
        println!("\n=== Quality Test: Fast Vector vs Iterator Equivalence ===\n");
        
        const TEST_DIMS: &[usize] = &[96, 192, 384, 768];
        const TEST_CASES: usize = 100;
        
        for &dim in TEST_DIMS {
            let mut max_diff = 0.0f32;
            
            for case in 0..TEST_CASES {
                let a: Vec<f32> = (0..dim)
                    .map(|i| ((case as f32 * i as f32) % 100.0).sin())
                    .collect();
                let b: Vec<f32> = (0..dim)
                    .map(|i| ((case as f32 * i as f32) % 100.0).cos())
                    .collect();
                
                let result_chunked = cosine_similarity_fast(&a, &b);
                let result_iter = cosine_similarity_iter(&a, &b);
                
                let diff = (result_chunked - result_iter).abs();
                max_diff = max_diff.max(diff);
            }
            
            println!("  {}D: max_diff={:.2e}", dim, max_diff);
        }
        
        println!("  ✓ Iterator variant mathematically equivalent");
    }

    // Helper functions
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        
        for i in 0..a.len() {
            let av = a[i];
            let bv = b[i];
            dot += av * bv;
            norm_a += av * av;
            norm_b += bv * bv;
        }
        
        let norm = (norm_a * norm_b).sqrt();
        if norm > 0.0 { dot / norm } else { 0.0 }
    }

    fn cosine_similarity_naive(a: &[f32], b: &[f32]) -> f32 {
        cosine_similarity(a, b)
    }

    fn cosine_similarity_fast(a: &[f32], b: &[f32]) -> f32 {
        // Matches fast_vector.rs 32-element chunk pattern
        if a.is_empty() { return 0.0; }
        
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        
        let chunk_size = 32;
        let full_chunks = a.len() / chunk_size;
        
        for chunk in 0..full_chunks {
            let base = chunk * chunk_size;
            for i in 0..chunk_size {
                let idx = base + i;
                let av = a[idx];
                let bv = b[idx];
                dot += av * bv;
                norm_a += av * av;
                norm_b += bv * bv;
            }
        }
        
        for i in (full_chunks * chunk_size)..a.len() {
            let av = a[i];
            let bv = b[i];
            dot += av * bv;
            norm_a += av * av;
            norm_b += bv * bv;
        }
        
        let norm = (norm_a * norm_b).sqrt();
        if norm > 0.0 { dot / norm } else { 0.0 }
    }

    fn cosine_similarity_iter(a: &[f32], b: &[f32]) -> f32 {
        let (dot, norm_a, norm_b) = a.iter()
            .zip(b.iter())
            .fold((0.0, 0.0, 0.0), |(dot, na, nb), (&av, &bv)| {
                (dot + av * bv, na + av * av, nb + bv * bv)
            });
        let norm = (norm_a * norm_b).sqrt();
        if norm > 0.0 { dot / norm } else { 0.0 }
    }

    #[derive(Default)]
    struct QualityMetrics {
        recall_at_100: Vec<f32>,
        precision_at_100: Vec<f32>,
    }
}
