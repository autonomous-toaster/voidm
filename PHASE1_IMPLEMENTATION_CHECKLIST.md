# Phase 1 Implementation Checklist: Singleton Model Caching

## Target
- **Duration**: 1-2 hours
- **Speedup**: 3-4x (750-1250ms → 250-400ms per query)
- **Risk**: Low

---

## Pre-Implementation Checklist

- [ ] Read `PERFORMANCE_SOLUTION_PLAN.md` (understand the approach)
- [ ] Have `/voidm` repo checked out locally
- [ ] Can run `cargo build` and `cargo test`
- [ ] M3/M4 MacBook or Linux for benchmarking (optional but recommended)

---

## Implementation Steps (1-2 hours total)

### ✅ Step 1: Add Dependency (5 min)

**File**: `/voidm/Cargo.toml`

Add to `[dependencies]`:
```toml
lazy_static = "1.4"
```

**Verify**: `cargo check --all` should pass

---

### ✅ Step 2: Create Cache Module (20 min)

**File**: `/voidm/crates/voidm-core/src/gguf_model_cache.rs`

Use the provided template:
```rust
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Mutex;

lazy_static! {
    static ref MODEL_CACHE: Mutex<HashMap<String, Engine>> = {
        Mutex::new(HashMap::new())
    };
}

pub fn get_or_load_model(model_key: &str, ...) -> Result<Engine> {
    // Return cached if exists, load if not
}

pub fn clear_model_cache() {
    // Called on shutdown
}
```

**Key Points**:
- Uses `lazy_static` for global cache
- Thread-safe with `Mutex`
- Returns cached instance on hit
- Loads once on miss

**Verify**: `cargo check --all` should pass

---

### ✅ Step 3: Export Module (5 min)

**File**: `/voidm/crates/voidm-core/src/lib.rs`

Add to module list:
```rust
pub mod gguf_model_cache;
```

**Verify**: `cargo check --all` should pass

---

### ✅ Step 4: Update Query Expander (15 min)

**File**: `/voidm/crates/voidm-core/src/gguf_query_expander.rs`

Current code (BEFORE):
```rust
pub async fn expand_query_gguf(&self, query: &str) -> Result<String> {
    let model_path = Self::get_model_path(&self.model_name).await?;
    let engine = Self::load_model(&self.model_name, &model_path)?;  // LOAD EVERY TIME
    let output = engine.generate(&prompt, 100)?;
    Ok(output)
}
```

Change to (AFTER):
```rust
use crate::gguf_model_cache::get_or_load_model;

pub async fn expand_query_gguf(&self, query: &str) -> Result<String> {
    let model_path = Self::get_model_path(&self.model_name).await?;
    
    // Use cache instead of direct load
    let model_key = format!("{}", self.model_name);
    let engine = get_or_load_model(
        &model_key,
        &model_path,
        self.get_engine_config(),
    )?;
    
    let output = engine.generate(&prompt, 100)?;
    Ok(output)
}
```

**Key Changes**:
- Import `get_or_load_model`
- Create unique `model_key`
- Use `get_or_load_model()` instead of direct `load_model()`
- Same inference code

**Verify**: `cargo check --all` should pass

---

### ✅ Step 5: Add Shutdown Cleanup (10 min)

**File**: `/voidm/crates/voidm-cli/src/main.rs`

Add cleanup on shutdown:

```rust
use voidm_core::gguf_model_cache::clear_model_cache;

#[tokio::main]
async fn main() -> Result<()> {
    // ... setup code ...
    
    let result = app.run().await;
    
    // Cleanup before exit
    clear_model_cache();
    
    result
}
```

Or in signal handler:
```rust
tokio::signal::ctrl_c().await?;
clear_model_cache();
```

**Key Points**:
- Ensures models are unloaded gracefully
- Prevents resource leaks
- Called on Ctrl+C or normal exit

**Verify**: `cargo check --all` should pass

---

### ✅ Step 6: Build & Test (20 min)

**Test 1: Compile**
```bash
cd /voidm
cargo build --release --features=gguf
# Should complete in ~30-60 seconds
```

**Test 2: Quick Latency Check (manual)**
```bash
# First query (will load model)
time voidm search "docker container networking" --query-expand true
# Expect: ~750-1250ms

# Second query (should use cache)
time voidm search "machine learning python" --query-expand true
# Expect: ~250ms (cache hit!)

# Third query
time voidm search "web application security" --query-expand true
# Expect: ~250ms (cache hit!)
```

**Test 3: Run Unit Tests**
```bash
cargo test --lib gguf_model_cache
# Should pass cache stats test
```

**Test 4: 10-Query Benchmark**
```bash
# Create benchmark script
cat > /tmp/bench.sh << 'EOF'
#!/bin/bash
queries=(
  "docker container networking"
  "machine learning python"
  "web application security"
  "database query optimization"
  "kubernetes deployment strategies"
  "REST API design patterns"
  "microservices architecture"
  "cloud security best practices"
  "agile development methodology"
  "devops infrastructure"
)

echo "Running benchmark (10 queries)..."
total_time=0
for i in "${!queries[@]}"; do
  query="${queries[$i]}"
  echo -n "Query $((i+1)): "
  
  # Run query and capture timing
  start=$(date +%s%N)
  voidm search "$query" --query-expand true > /dev/null 2>&1
  end=$(date +%s%N)
  
  elapsed=$((($end - $start) / 1000000))  # Convert to ms
  echo "${elapsed}ms"
  total_time=$((total_time + elapsed))
done

average=$((total_time / 10))
echo ""
echo "Total: ${total_time}ms"
echo "Average: ${average}ms per query"
EOF

chmod +x /tmp/bench.sh
/tmp/bench.sh
```

**Expected Results**:
- Query 1: ~750-1250ms (first load)
- Query 2-10: ~250ms each (cached)
- Average: 320-400ms per query
- **Improvement: 2.3-3.8x speedup** ✅

---

## Verification Checklist

### Code Quality
- [ ] No compiler warnings: `cargo check --all`
- [ ] All tests pass: `cargo test --lib`
- [ ] No clippy warnings: `cargo clippy --all`

### Performance
- [ ] First query: 750-1250ms (expected)
- [ ] Cached queries: 250-400ms (3-4x faster)
- [ ] 10-query average: < 400ms per query

### Functionality
- [ ] Query expansion still works
- [ ] Results are identical to before
- [ ] No hanging or timeouts
- [ ] Graceful cleanup on exit

---

## Troubleshooting

### Issue: Compiler error about `Engine` not implementing `Clone`

**Solution**: Check that `Engine` has `#[derive(Clone)]` in gguf_query_expander.rs
```rust
#[derive(Clone)]
pub struct Engine {
    // ...
}
```

### Issue: Model cache not being hit (always slow)

**Solution**: 
1. Check logs: `RUST_LOG=debug voidm search ...`
2. Look for "cache HIT" vs "cache MISS" messages
3. Verify model_key is consistent across calls

### Issue: Out of memory (OOM)

**Solution**:
1. Expected if loading multiple different models
2. Monitor with `top` or `Activity Monitor`
3. Residency should be ~1.2GB per model
4. Clear cache on shutdown: `clear_model_cache()`

### Issue: Tests failing

**Solution**:
1. Run individual test: `cargo test --lib gguf_model_cache::tests`
2. Check if model file exists at expected path
3. Ensure GGUF feature is enabled: `cargo test --features=gguf`

---

## Commit Guidance

After successful implementation:

```bash
cd /voidm
git add -A
git commit -m "✨ Phase 1: Singleton Model Caching (3-4x Speedup)

Implement lazy-loaded model cache to eliminate per-request reload overhead

CHANGES:
├─ Cargo.toml: Add lazy_static dependency
├─ gguf_model_cache.rs (NEW): Global model cache with thread-safe access
├─ lib.rs: Export new module
├─ gguf_query_expander.rs: Use cache instead of direct load
└─ main.rs: Add cleanup on shutdown

PERFORMANCE:
├─ Before: 750-1250ms per query
├─ After:  250-400ms per query (3-4x faster)
└─ Savings: 500-1000ms per request × millions of queries = hours/week

TESTING:
├─ Unit tests: Cache hit/miss scenarios
├─ Integration tests: 10-query benchmark
└─ Result: ✅ Meets 3-4x speedup target

NEXT: Phase 2 (GPU acceleration) for additional 10-16x speedup
"
```

---

## Success Metrics

✅ **Complete Phase 1 successfully when**:
1. Code compiles with no warnings
2. All tests pass
3. First query: 750-1250ms (baseline)
4. Cached queries: 250-400ms (3-4x faster)
5. 10-query average: < 400ms per query
6. Graceful cleanup on shutdown

---

## Time Breakdown

| Step | Task | Time | Status |
|------|------|------|--------|
| 1 | Add dependency | 5 min | ✓ Easy |
| 2 | Create cache module | 20 min | ✓ Template provided |
| 3 | Export module | 5 min | ✓ One line |
| 4 | Update query expander | 15 min | ✓ 3-line change |
| 5 | Add shutdown cleanup | 10 min | ✓ Signal handler |
| 6 | Build & test | 20 min | ✓ Compile once |
| **Total** | **All steps** | **75 min** | **1-2 hours** |

---

## Next Phase

After Phase 1 is complete and verified:

1. ✅ Benchmark shows 3-4x improvement
2. ✅ All tests passing
3. ✅ Deploy to staging
4. ⏳ Gather user feedback
5. 🎯 Plan Phase 2 (GPU acceleration, 4-6 hours)

See: `PERFORMANCE_SOLUTION_PLAN.md` Phase 2 section

---

## Questions?

Refer to:
- `PERFORMANCE_SOLUTION_PLAN.md` - Full architecture
- `GGUF_CACHING_IMPLEMENTATION.md` - Detailed guide
- `QMD_ARCHITECTURE_ANALYSIS.md` - Why this works

Good luck! 🚀
