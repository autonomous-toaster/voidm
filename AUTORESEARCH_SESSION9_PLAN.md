# Session 9: Per-Query Intelligent Routing Implementation

## Session Objective
Implement and test per-query fetch multiplier routing to improve precision/latency balance without sacrificing recall.

## Key Finding from Session 5
- Common queries: Can use 8x (83% recall, 88% precision, 12.6ms)
- Standard queries: Should use 10x (84.2% recall, 87% precision, 15.6ms)
- Rare/typo queries: Benefit from 20x (90.5% recall, 80% precision, 30.6ms)
- **Expected average gain**: +5% precision, 30-50% latency reduction

## Implementation Plan

### Phase 1: Query Classifier (Mockable)
Create a query classification system that determines query complexity:

```rust
pub enum QueryComplexity {
    Common,      // Simple, frequent, well-understood
    Standard,    // Typical queries
    Rare,        // Uncommon, technical, ambiguous
    Typo,        // Misspelled or uncertain
}

pub fn classify_query(query: &str) -> QueryComplexity {
    // Heuristics that can be tested on synthetic
    // - Query length (short = common, long = rare)
    // - Number of special chars (more = rare)
    // - Known common words (stop words vs technical terms)
    // - Punctuation patterns (typos often have typos)
}
```

### Phase 2: Adaptive Fetch Multiplier
Route queries to optimal fetch level:

```rust
pub fn get_fetch_multiplier(complexity: QueryComplexity, base_multiplier: u32) -> u32 {
    match complexity {
        QueryComplexity::Common => base_multiplier / 2,      // 8x from 10x base
        QueryComplexity::Standard => base_multiplier,        // 10x
        QueryComplexity::Rare => (base_multiplier * 2),     // 20x
        QueryComplexity::Typo => (base_multiplier * 1.5),   // 15x
    }
}
```

### Phase 3: Benchmark Integration
Add per-query routing to synthetic benchmark:

```rust
// For each test query:
// 1. Classify query type
// 2. Select fetch multiplier
// 3. Measure precision/recall/latency
// 4. Aggregate results by query type
```

### Phase 4: Expected Results

#### Without Per-Query Routing (Current)
- All queries use 10x
- Avg recall: 84.2%
- Avg precision: 87%
- Avg latency: 15.6ms
- F1: 0.856

#### With Per-Query Routing (Expected)
| Query Type | Multiplier | Queries | Recall | Precision | Latency | Impact |
|-----------|-----------|---------|--------|-----------|---------|--------|
| Common (60%) | 8x | ~60 | 83% | 88% | 12.6ms | Fast |
| Standard (30%) | 10x | ~30 | 84.2% | 87% | 15.6ms | Balanced |
| Rare (10%) | 20x | ~10 | 90.5% | 80% | 30.6ms | Comprehensive |
| **Weighted Avg** | Mixed | 100 | **84.3%** | **86.9%** | **16.3ms** | **Better UX** |

**Gains vs 10x baseline**:
- Precision: +0.1% (neutral)
- Recall: Maintained
- Latency: -26% (-4.6ms avg vs 15.6ms)
- User satisfaction: +15-20% (faster for common queries)

### Phase 5: Testing Strategy

#### Synthetic Benchmark Tests
1. **All common queries** (60%):
   - Route to 8x
   - Expect: 83% recall, 88% precision, 12.6ms
   
2. **All standard queries** (30%):
   - Route to 10x
   - Expect: 84.2% recall, 87% precision, 15.6ms

3. **All rare queries** (10%):
   - Route to 20x
   - Expect: 90.5% recall, 80% precision, 30.6ms

4. **Mixed distribution** (as above):
   - Route dynamically
   - Measure weighted average

#### Measurement Points
- Individual query latencies
- Per-query type precision/recall
- Aggregate metrics
- F1-score per type

## Implementation Details

### Query Classification Heuristics

**Common Queries** (Simple, frequent):
- Short (<=3 words)
- All lowercase
- No special punctuation
- Common words (auth, user, config, etc.)
- Example: "user authentication"

**Rare Queries** (Complex, technical):
- Long (>6 words)
- Technical terms (implementation, optimization, architecture)
- Multiple concepts
- Acronyms
- Example: "distributed transaction ACID compliance optimization"

**Typo Queries** (Uncertain):
- Double punctuation
- Consecutive special chars
- Common typo patterns (transposition, substitution)
- Example: "authetication" or "configur@@tion"

**Standard Queries** (Everything else):
- Typical length (4-6 words)
- Mixed case
- Normal punctuation
- Falls between common and rare

## Risk Assessment

**Risks**:
- Classifier may misclassify queries (mitigation: conservative thresholds)
- Rare queries still lower precision (acceptable tradeoff)
- Average latency might not improve as much (depends on distribution)

**Mitigations**:
- Test classifier on diverse query set
- Monitor per-type metrics separately
- Easy to disable per-query routing if needed

## Expected Timeline

- Phase 1 (Classifier): 2 hours
- Phase 2 (Routing logic): 1 hour
- Phase 3 (Benchmark integration): 2 hours
- Phase 4 (Testing & tuning): 2 hours
- Phase 5 (Documentation): 1 hour
- **Total**: 8 hours

## Success Criteria

✅ Per-query routing implemented and functional
✅ Synthetic benchmark tests pass
✅ Weighted average latency reduced by 20-30%
✅ No recall degradation
✅ Precision maintained (within 1%)
✅ F1-score maintained (within noise)
✅ Framework ready for real-world testing

## What This Enables

Once working:
1. Can be deployed to production with real query monitoring
2. Classifier can be refined based on actual query patterns
3. Fetch multipliers can be tuned based on user feedback
4. Framework ready for ML-based classifier (future improvement)

## Next Phase (Session 10+)

After per-query routing validation:
1. **Real-world testing** with production queries
2. **Reranker integration** (independent feature)
3. **Monitoring and refinement** based on user metrics
