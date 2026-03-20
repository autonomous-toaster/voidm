//! Compare different vector similarity implementations
//! Focus on mathematical equivalence and relative performance

#[cfg(test)]
mod comparison_benchmark {
    use voidm_core::fast_vector;
    use voidm_core::fast_vector_iter;
    use std::time::Instant;

    #[test]
    fn test_vector_methods_equivalence_and_performance() {
        println!("\n=== Vector Similarity Implementation Comparison ===\n");
        
        const DIMS: &[usize] = &[96, 192, 384, 768, 1024];
        const ITERATIONS: usize = 10000;
        
        for &dim in DIMS {
            let query: Vec<f32> = (0..dim).map(|i| (i as f32).sin()).collect();
            let docs: Vec<Vec<f32>> = (0..100)
                .map(|d_idx| {
                    (0..dim)
                        .map(|i| ((d_idx as f32 + i as f32) % 100.0).cos())
                        .collect()
                })
                .collect();
            
            // Method 1: Current chunked loops (fast_vector.rs)
            let start = Instant::now();
            for _ in 0..ITERATIONS {
                for doc in &docs {
                    let _ = fast_vector::cosine_similarity(&query, doc);
                }
            }
            let time_chunked = start.elapsed().as_micros();
            
            // Method 2: Simple iterator (fold)
            let start = Instant::now();
            for _ in 0..ITERATIONS {
                for doc in &docs {
                    let _ = fast_vector_iter::cosine_similarity_iter(&query, doc);
                }
            }
            let time_iter = start.elapsed().as_micros();
            
            // Method 3: Unrolled iterator (chunks_exact)
            let start = Instant::now();
            for _ in 0..ITERATIONS {
                for doc in &docs {
                    let _ = fast_vector_iter::cosine_similarity_iter_unrolled(&query, doc);
                }
            }
            let time_iter_unrolled = start.elapsed().as_micros();
            
            // Verify equivalence (spot check)
            let test_doc = &docs[0];
            let res_chunked = fast_vector::cosine_similarity(&query, test_doc);
            let res_iter = fast_vector_iter::cosine_similarity_iter(&query, test_doc);
            let res_iter_unrolled = fast_vector_iter::cosine_similarity_iter_unrolled(&query, test_doc);
            
            let err1 = (res_chunked - res_iter).abs();
            let err2 = (res_chunked - res_iter_unrolled).abs();
            
            let speedup_iter = time_chunked as f32 / time_iter as f32;
            let speedup_unrolled = time_chunked as f32 / time_iter_unrolled as f32;
            
            println!("{}D ({} iters, 100 docs):", dim, ITERATIONS);
            println!("  Chunked:         {:.0} µs  (baseline)", time_chunked / (ITERATIONS as u128 * 100));
            println!("  Iterator:        {:.0} µs  ({:.2}x {})", 
                     time_iter / (ITERATIONS as u128 * 100),
                     speedup_iter,
                     if speedup_iter > 1.0 { "faster" } else { "slower" });
            println!("  Iterator+Unroll: {:.0} µs  ({:.2}x {})",
                     time_iter_unrolled / (ITERATIONS as u128 * 100),
                     speedup_unrolled,
                     if speedup_unrolled > 1.0 { "faster" } else { "slower" });
            println!("  Error (chunked vs iter):          {:.2e}", err1);
            println!("  Error (chunked vs iter+unroll):  {:.2e}", err2);
            println!();
        }
    }
}
