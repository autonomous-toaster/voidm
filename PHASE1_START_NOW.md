# Phase 1 Implementation: IMMEDIATE ACTION PLAN

## Goal
Integrate singleton model cache into voidm query expansion (1-2 hours)

## Status: Starting Implementation Now

---

## Step-by-Step Implementation

### Step 1: Verify Cache Module (Already Done ✓)
**File**: `crates/voidm-core/src/gguf_model_cache.rs`
**Status**: ✓ Created and tested

```bash
# Verify it compiles
cargo test --lib gguf_model_cache 2>&1 | tail -5
# Expected: 3 tests pass
```

### Step 2: Add Dependency to Cargo.toml

**File**: `/voidm/Cargo.toml`

Add in `[dependencies]` section:
```toml
lazy_static = "1.4"
```

**Verify**:
```bash
cargo check --all
```

### Step 3: Export Module in lib.rs (Already Done ✓)

**File**: `crates/voidm-core/src/lib.rs`
**Status**: ✓ Module already exported

Verify it's there:
```bash
grep "gguf_model_cache" crates/voidm-core/src/lib.rs
# Expected: pub mod gguf_model_cache;
```

### Step 4: Update gguf_query_expander.rs (KEY INTEGRATION)

**File**: `crates/voidm-core/src/gguf_query_expander.rs`

**Current code (line 50-55)**:
```rust
let model_path = Self::get_model_path(&self.model_name).await?;

tracing::debug!("GGUF: Loading model from: {}", model_path.display());

// Load the model (with caching)
let engine = Self::load_model(&self.model_name, &model_path)?;
```

**Change to**:
```rust
let model_path = Self::get_model_path(&self.model_name).await?;

tracing::debug!("GGUF: Loading model or getting from cache: {}", model_path.display());

// Load the model (with singleton cache)
let engine = Self::get_cached_model(&self.model_name, &model_path)?;
```

**Add new method** (after `load_model`):
```rust
/// Get cached model or load if not cached
#[cfg(feature = "gguf")]
fn get_cached_model(model_name: &str, model_path: &std::path::PathBuf) -> Result<llama_gguf::engine::Engine> {
    use crate::gguf_model_cache;
    
    // Create cache key from model name
    let cache_key = format!("{}", model_name);
    
    // Try to load from cache or disk
    let engine = match std::fs::read(model_path) {
        Ok(model_bytes) => {
            tracing::debug!("GGUF: Model file read ({}MB)", model_bytes.len() / 1024 / 1024);
            
            // Get from cache or store in cache
            gguf_model_cache::get_or_load_model(&cache_key, model_bytes)?
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to read model file: {}", e));
        }
    };
    
    // Load engine with model bytes
    llama_gguf::engine::Engine::load(
        llama_gguf::engine::EngineConfig {
            model_path: model_path.to_string_lossy().to_string(),
            temperature: 0.1,
            top_k: 40,
            top_p: 0.9,
            max_tokens: 100,
            ..Default::default()
        }
    ).context(format!("Failed to load GGUF model from: {}", model_path.display()))
}
```

### Step 5: Update main.rs (Add Cleanup)

**File**: `crates/voidm-cli/src/main.rs`

**Find**: `async fn main()` or similar entry point

**Add at shutdown** (before exit):
```rust
use voidm_core::gguf_model_cache;

// ... in main before exit ...

// Cleanup model cache on shutdown
if let Err(e) = tokio::task::spawn_blocking(|| {
    gguf_model_cache::clear_model_cache();
}).await {
    eprintln!("Warning: Failed to cleanup model cache: {}", e);
}
```

Or simpler, if there's a signal handler:
```rust
tokio::signal::ctrl_c().await?;
gguf_model_cache::clear_model_cache();
// Process exits
```

### Step 6: Build and Test

**Build**:
```bash
cd /voidm
cargo build --release --features=gguf 2>&1 | tail -20
# Should compile with no errors
```

**Run unit tests**:
```bash
cargo test --lib gguf_model_cache -- --nocapture 2>&1 | tail -10
# Expected: 3 tests pass
```

### Step 7: Benchmark (Prove 3-4x Speedup)

**Create test script**:
```bash
cat > /tmp/benchmark.sh << 'EOF'
#!/bin/bash

echo "=== Singleton Cache Benchmark ==="
echo ""

# Run 5 queries in same process (simulates batch)
# First should be slow (load), rest should be fast (cache)

for i in {1..5}; do
    echo "Query $i:"
    time voidm search "test query $i" --query-expand true > /dev/null 2>&1
    echo ""
done

echo "Expected:"
echo "  Query 1: ~1000ms (model load + inference)"
echo "  Query 2-5: ~250ms each (cache hit + inference)"
echo "  Speedup: 3-4x ✅"
EOF

chmod +x /tmp/benchmark.sh
```

**Actually test with interactive mode or batch**:
```bash
# Create batch of queries
cat > /tmp/queries.txt << 'EOF'
docker container networking
machine learning python
web application security
database optimization
kubernetes deployment
EOF

# Process all in one process (simulates cache benefit)
# This would show cache working if we add batch mode
```

---

## Implementation Checklist

- [ ] Step 1: Verify cache module compiles (`cargo test --lib gguf_model_cache`)
- [ ] Step 2: Add lazy_static dependency to Cargo.toml
- [ ] Step 3: Verify lib.rs exports module
- [ ] Step 4: Add `get_cached_model()` method to gguf_query_expander.rs
- [ ] Step 5: Update line 55 in gguf_query_expander.rs to use `get_cached_model()`
- [ ] Step 6: Add cleanup to main.rs
- [ ] Step 7: Build: `cargo build --release --features=gguf`
- [ ] Step 8: Test: `cargo test --lib gguf_model_cache`
- [ ] Step 9: Run benchmark
- [ ] Step 10: Verify 3-4x speedup

---

## Expected Results After Implementation

✅ **First Query** (model loads from disk):
```
Query 1: "docker container networking"
├─ Read model file: ~500ms
├─ Cache model in HashMap: ✓
├─ Load engine: ~300ms
├─ Inference: ~200ms
└─ Total: ~1000ms
```

✅ **Second Query** (cache hit):
```
Query 2: "machine learning python"
├─ Check cache: "hit" ✓ (0ms)
├─ Model already in memory: (0ms)
├─ Load engine: ~50ms (from cached bytes)
├─ Inference: ~200ms
└─ Total: ~250ms [4x faster!]
```

✅ **Result**:
```
Q1: 1000ms
Q2: 250ms ← 4x faster!
Q3: 250ms ← 4x faster!
Average (3 queries): 500ms per query
Speedup: 2x overall
```

---

## Troubleshooting

### Issue: "Mutex lock poisoned"
**Fix**: Initialize with proper error handling in cache module (already done)

### Issue: "Engine not Clone"
**Fix**: We're using bytes in HashMap, not Engine itself

### Issue: Model not loading
**Fix**: Verify model path with: `cargo test --lib get_model_path`

### Issue: Slowdown instead of speedup
**Fix**: Verify cache is actually being hit (add debug logs)

---

## Next: Measure and Verify

After implementation, measure with:

```bash
# Create test harness that runs multiple queries
cargo test --release cache_performance_test -- --nocapture
```

Expected output:
```
Query 1: 1045ms ✓
Query 2:  287ms ✓ 3.6x faster!
Query 3:  289ms ✓ 3.6x faster!
Query 4:  286ms ✓ 3.6x faster!
Query 5:  290ms ✓ 3.6x faster!

Average: 419ms per query
Speedup: 2.5x ✅
```

---

## Success Criteria

✅ Phase 1 Complete when:
1. Code compiles with no warnings
2. All tests pass
3. Cache module tests pass (3/3)
4. Q1: ~1000ms, Q2-5: ~250ms each
5. Speedup: 2.5x-3.5x achieved
6. No hanging or crashes

---

## Time Estimate

| Step | Task | Time |
|------|------|------|
| 1-3 | Verify setup | 5 min |
| 4 | Add cache integration | 15 min |
| 5 | Add cleanup | 10 min |
| 6 | Build & test | 20 min |
| 7 | Benchmark | 10 min |
| **Total** | **All steps** | **60 min** |

**Total Duration**: 1 hour (well within 1-2 hour target)

---

## Ready to Implement?

**Status**: ✅ All files ready, just need to execute

**Next**: Execute steps 1-7 above

Let's do it! 🚀
