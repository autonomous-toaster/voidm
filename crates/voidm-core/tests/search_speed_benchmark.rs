//! Real search performance benchmark with full features
//! Run: cargo test --test search_speed_benchmark --release -- --nocapture --test-threads=1

#[cfg(test)]
mod search_speed_benchmark {
    use std::time::Instant;
    use std::collections::HashMap;
    use voidm_core::fast_vector;
    use voidm_core::approx_search::ApproximateSearcher;

    #[test]
    fn test_vector_similarity_fast_vs_naive() {
        println!("\n=== Vector Similarity: Fast vs Naive ===\n");
        
        let dimensions = vec![96, 192, 384, 768, 1024];
        let doc_count = 10000;
        
        for dim in dimensions {
            let query_vec = vec![0.5; dim];
            let doc_vecs = (0..doc_count)
                .map(|i| {
                    let mut v = vec![0.5; dim];
                    v[i % dim] = 0.7;
                    v
                })
                .collect::<Vec<_>>();
            
            // Naive approach
            let start = Instant::now();
            let _naive: Vec<f32> = doc_vecs.iter()
                .map(|doc_vec| {
                    let dot: f32 = query_vec.iter().zip(doc_vec.iter()).map(|(a, b)| a * b).sum();
                    let qa: f32 = query_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let db: f32 = doc_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                    dot / (qa * db + 1e-8)
                })
                .collect();
            let naive_us = start.elapsed().as_micros();
            
            // Fast approach
            let start = Instant::now();
            let _fast: Vec<f32> = doc_vecs.iter()
                .map(|doc_vec| fast_vector::cosine_similarity(&query_vec, doc_vec))
                .collect();
            let fast_us = start.elapsed().as_micros();
            
            let speedup = naive_us as f32 / fast_us as f32;
            println!("  {}D: Naive {} µs, Fast {} µs ({:.1}x speedup)", 
                     dim, naive_us, fast_us, speedup);
        }
    }

    #[test]
    fn test_full_search_pipeline_optimized() {
        println!("\n=== Full Search Pipeline (Optimized) ===\n");
        
        let search_count = 1000;
        let doc_count = 5000;
        let query_dim = 384;
        
        let query_vec = vec![0.5; query_dim];
        let doc_vecs = (0..doc_count)
            .map(|i| {
                let mut v = vec![0.5; query_dim];
                v[i % query_dim] = 0.7;
                v
            })
            .collect::<Vec<_>>();
        
        let start = Instant::now();
        
        for _search_id in 0..search_count {
            let mut scores = HashMap::new();
            
            // Vector similarity with optimized function
            for (i, doc_vec) in doc_vecs.iter().enumerate() {
                let sim = fast_vector::cosine_similarity(&query_vec, doc_vec);
                scores.insert(i as u32, sim);
            }
            
            // RRF merging
            let mut merged: HashMap<u32, f32> = HashMap::new();
            for (doc_id, score) in scores {
                *merged.entry(doc_id).or_insert(0.0) += score;
            }
            
            // Sort and get top results
            let mut results: Vec<_> = merged.iter().collect();
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let _top_10 = results.iter().take(10).collect::<Vec<_>>();
        }
        
        let elapsed_ms = start.elapsed().as_millis();
        let throughput = (search_count as f64 * 1000.0) / elapsed_ms as f64;
        let latency = elapsed_ms as f64 / search_count as f64;
        
        println!("  {} searches over {} docs", search_count, doc_count);
        println!("    Total: {} ms", elapsed_ms);
        println!("    Avg latency: {:.2} ms/search", latency);
        println!("    Throughput: {:.1} searches/sec", throughput);
    }

    #[test]
    fn test_approximate_search_speedup() {
        println!("\n=== Approximate Search: Early Termination ===\n");
        
        let doc_count = 10000;
        let query_dim = 384;
        let query = vec![0.5; query_dim];
        let documents: Vec<_> = (0..doc_count)
            .map(|i| {
                let mut v = vec![0.5; query_dim];
                v[i % query_dim] = 0.7;
                v
            })
            .collect();
        
        let searcher = ApproximateSearcher::new();
        let k = 10;
        
        // Approximate (with early termination)
        let start = Instant::now();
        let _approx = searcher.approx_search_top_k(&query, &documents, k);
        let approx_ms = start.elapsed().as_millis();
        
        // Full search (exhaustive)
        let start = Instant::now();
        let _full = ApproximateSearcher::full_search_top_k(&query, &documents, k);
        let full_ms = start.elapsed().as_millis();
        
        let speedup = full_ms as f32 / approx_ms.max(1) as f32;
        println!("  Approximate: {} ms", approx_ms);
        println!("  Full search: {} ms", full_ms);
        println!("  Speedup: {:.1}x", speedup);
    }

    #[test]
    fn test_end_to_end_latency() {
        println!("\n=== End-to-End Search Latency ===\n");
        
        // Simulate realistic search with all optimizations
        let scenarios = vec![
            ("Small (100 docs)", 100),
            ("Medium (1K docs)", 1000),
            ("Large (10K docs)", 10000),
            ("XL (50K docs)", 50000),
        ];
        
        let query_dim = 384;
        let query = vec![0.5; query_dim];
        
        for (name, doc_count) in scenarios {
            let doc_vecs = (0..doc_count)
                .map(|i| {
                    let mut v = vec![0.5; query_dim];
                    v[i % query_dim] = 0.7;
                    v
                })
                .collect::<Vec<_>>();
            
            let start = Instant::now();
            
            // Vector similarity pass (optimized)
            let mut scores = HashMap::new();
            for (i, doc_vec) in doc_vecs.iter().enumerate() {
                let sim = fast_vector::cosine_similarity(&query, doc_vec);
                scores.insert(i as u32, sim);
            }
            
            // Merge and rank
            let mut merged: HashMap<u32, f32> = HashMap::new();
            for (doc_id, score) in scores {
                *merged.entry(doc_id).or_insert(0.0) += score;
            }
            
            let mut results: Vec<_> = merged.iter().collect();
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            
            let elapsed_ms = start.elapsed().as_millis();
            
            println!("  {}: {} ms", name, elapsed_ms);
        }
    }
}
