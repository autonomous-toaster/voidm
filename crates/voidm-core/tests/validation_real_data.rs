// Real data validation test for Phase A chunking algorithm
// Tests smart vs naive chunking on actual memories from SQLite

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use voidm_core::chunking::{chunk_smart, ChunkingStrategy};
    use voidm_core::coherence::estimate_coherence;

    #[derive(Debug, Clone)]
    struct Memory {
        id: String,
        content: String,
        memory_type: String,
    }

    fn get_db_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join("Library/Application Support/voidm/memories.db")
    }

    fn load_real_memories(limit: usize) -> Result<Vec<Memory>, Box<dyn std::error::Error>> {
        let db_path = get_db_path();
        
        // Use sqlite3 command-line tool to query
        let output = std::process::Command::new("sqlite3")
            .arg(&db_path)
            .arg(&format!("SELECT id, content, type FROM memories ORDER BY RANDOM() LIMIT {};", limit))
            .output()?;

        if !output.status.success() {
            return Err("Failed to query SQLite".into());
        }

        let mut memories = Vec::new();
        let stdout = String::from_utf8(output.stdout)?;
        
        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() == 3 {
                memories.push(Memory {
                    id: parts[0].to_string(),
                    content: parts[1].to_string(),
                    memory_type: parts[2].to_string(),
                });
            }
        }

        Ok(memories)
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored --nocapture
    fn test_smart_chunking_on_real_data() {
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE A VALIDATION: Smart Chunking on Real Data               ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        // Load 10 real memories
        let memories = match load_real_memories(10) {
            Ok(m) => m,
            Err(e) => {
                println!("⚠️  Failed to load memories: {}", e);
                println!("Make sure SQLite is installed and database exists at:");
                println!("  ~/Library/Application Support/voidm/memories.db");
                return;
            }
        };
        
        if memories.is_empty() {
            println!("⚠️  No memories found in SQLite database");
            println!("Expected: ~/Library/Application Support/voidm/memories.db");
            return;
        }

        println!("Loaded {} real memories from SQLite", memories.len());
        println!("{}\n", "=".repeat(70));

        let strategy = ChunkingStrategy::default();
        let mut smart_stats = Vec::new();
        let mut total_smart_coherence = 0.0;
        let mut total_chunks = 0usize;

        for (idx, memory) in memories.iter().enumerate() {
            println!("\n[Memory {}] {}", idx + 1, memory.id);
            println!("  Type: {}", memory.memory_type);
            println!("  Size: {} chars", memory.content.len());
            println!("  Preview: {}...", 
                &memory.content[..60.min(memory.content.len())].replace('\n', " "));

            // Smart chunking
            match chunk_smart(&memory.id, &memory.content, &strategy, &memory.created_at) {
                Ok(chunks) => {
                    println!("  ✅ Smart chunking: {} chunks", chunks.len());
                    
                    let mut memory_coherence = 0.0;
                    let mut coherences = Vec::new();

                    for (chunk_idx, chunk) in chunks.iter().enumerate() {
                        let score = estimate_coherence(&chunk.content);
                        let final_score = score.final_score();
                        coherences.push(final_score);
                        memory_coherence += final_score;
                        
                        println!("    Chunk {}: {} chars, coherence {:.2} {}",
                            chunk_idx,
                            chunk.size,
                            final_score,
                            score.quality_level());
                    }

                    let avg_coherence = if chunks.is_empty() { 
                        0.0 
                    } else { 
                        memory_coherence / chunks.len() as f32 
                    };

                    println!("  Avg coherence: {:.2}", avg_coherence);
                    smart_stats.push((memory.id.clone(), avg_coherence, chunks.len()));
                    total_smart_coherence += avg_coherence;
                    total_chunks += chunks.len();

                    // Alert if low coherence
                    if avg_coherence < 0.75 {
                        println!("  ⚠️  WARNING: Low coherence (< 0.75)");
                    }
                }
                Err(e) => {
                    println!("  ❌ Smart chunking failed: {}", e);
                }
            }
        }

        // Summary
        println!("\n{}", "=".repeat(70));
        println!("\n📊 SUMMARY STATISTICS\n");

        let avg_smart_coherence = if smart_stats.is_empty() { 
            0.0 
        } else { 
            total_smart_coherence / smart_stats.len() as f32 
        };

        println!("Total memories tested: {}", smart_stats.len());
        println!("Total chunks created: {}", total_chunks);
        println!("Avg chunks per memory: {:.1}", 
            if smart_stats.is_empty() { 0.0 } else { total_chunks as f32 / smart_stats.len() as f32 });
        println!("Average coherence: {:.2}", avg_smart_coherence);

        // Count by quality level
        let excellent = smart_stats.iter().filter(|(_, c, _)| *c >= 0.8).count();
        let good = smart_stats.iter().filter(|(_, c, _)| *c >= 0.6 && *c < 0.8).count();
        let fair = smart_stats.iter().filter(|(_, c, _)| *c >= 0.3 && *c < 0.6).count();
        let poor = smart_stats.iter().filter(|(_, c, _)| *c < 0.3).count();

        println!("\nQuality distribution:");
        println!("  🟣 EXCELLENT (0.80+): {} ({:.0}%)", excellent, excellent as f32 / smart_stats.len() as f32 * 100.0);
        println!("  🟢 GOOD (0.60-0.79): {} ({:.0}%)", good, good as f32 / smart_stats.len() as f32 * 100.0);
        println!("  🟡 FAIR (0.30-0.59): {} ({:.0}%)", fair, fair as f32 / smart_stats.len() as f32 * 100.0);
        println!("  🔴 POOR (<0.30): {} ({:.0}%)", poor, poor as f32 / smart_stats.len() as f32 * 100.0);

        println!("\n{}", "=".repeat(70));

        // Validation result
        if avg_smart_coherence >= 0.75 {
            println!("\n✅ VALIDATION PASSED");
            println!("Average coherence {:.2} meets target of 0.75+", avg_smart_coherence);
            println!("Algorithm is ready for Part D (chunking 900 memories)");
        } else if avg_smart_coherence >= 0.60 {
            println!("\n⚠️  VALIDATION MARGINAL");
            println!("Average coherence {:.2} below target of 0.75", avg_smart_coherence);
            println!("Algorithm works but quality is mediocre");
            println!("Consider: adjust parameters or add special content handling");
        } else {
            println!("\n❌ VALIDATION FAILED");
            println!("Average coherence {:.2} is too low", avg_smart_coherence);
            println!("Algorithm needs redesign before Part D");
        }

        println!("\n{}", "=".repeat(70));
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored --nocapture
    fn test_smart_vs_naive_chunking() {
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║  A/B TEST: Smart vs Naive Chunking on Real Data                ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        let memories = match load_real_memories(5) {
            Ok(m) => m,
            Err(e) => {
                println!("Failed to load memories: {}", e);
                return;
            }
        };
        
        if memories.is_empty() {
            println!("No memories found");
            return;
        }

        let strategy = ChunkingStrategy::default();
        let mut smart_total = 0.0;
        let mut naive_total = 0.0;
        let mut count = 0;

        for memory in memories.iter() {
            println!("\nMemory: {} ({} chars)", memory.id, memory.content.len());

            // Smart chunking
            if let Ok(smart_chunks) = chunk_smart(&memory.id, &memory.content, &strategy, &memory.created_at) {
                let smart_avg: f32 = smart_chunks.iter()
                    .map(|c| estimate_coherence(&c.content).final_score())
                    .sum::<f32>() / smart_chunks.len().max(1) as f32;
                
                println!("  Smart: {} chunks, avg coherence {:.2}", smart_chunks.len(), smart_avg);
                smart_total += smart_avg;
            }

            // Naive chunking  
            if let Ok(naive_chunks) = chunk_smart(&memory.id, &memory.content, 
                &ChunkingStrategy { smart_breaks: false, ..Default::default() }) {
                let naive_avg: f32 = naive_chunks.iter()
                    .map(|c| estimate_coherence(&c.content).final_score())
                    .sum::<f32>() / naive_chunks.len().max(1) as f32;
                
                println!("  Naive: {} chunks, avg coherence {:.2}", naive_chunks.len(), naive_avg);
                naive_total += naive_avg;
                
                count += 1;
            }
        }

        if count > 0 {
            let smart_avg = smart_total / count as f32;
            let naive_avg = naive_total / count as f32;
            let improvement = ((smart_avg - naive_avg) / naive_avg.max(0.01)) * 100.0;

            println!("\n{}", "=".repeat(70));
            println!("\nSUMMARY:");
            println!("  Smart avg: {:.2}", smart_avg);
            println!("  Naive avg: {:.2}", naive_avg);
            println!("  Improvement: {:.1}%", improvement);

            if smart_avg > naive_avg {
                println!("\n✅ Smart chunking is better than naive!");
            } else {
                println!("\n⚠️  Naive chunking performed as well or better");
            }
        }
    }
}
