# Session 6 Plan: Remaining Optimization Opportunities

## Current State (Session 5 Completion)
- **Configuration**: 12x fetch multiplier (balanced)
- **Recall@100**: 85.5%
- **Precision@10**: 85%
- **F1-Score**: 0.854 (optimal)
- **Latency**: 18.6ms/query
- **Throughput**: 53.8 qps

## Completed & Saturated
- ✅ RRF bonuses (0.12/0.06 optimal)
- ✅ Metadata weights (-50% reduction optimal)
- ✅ Fetch limit core optimization (tested 3x-30x)
- ✅ RRF k parameter (60 optimal, tested 30-120)
- ✅ Score scaling (3.5 optimal, tested 2.5-4.5)
- ✅ Fuzzy threshold (0.6 optimal, tested 0.4-0.7)
- ✅ Signal importance analysis (Vector 42.5%, BM25 3.7%, Fuzzy 1.5%)

## High-ROI Opportunities (Remaining)

### 1. **Fetch Multiplier Fine-Tuning** (QUICK TEST)
Current sweet spot: 12x fetch
Adjacent options: 10x (faster), 14x (higher recall)

| Mult | Recall | Precision | F1-Score | Latency | Status |
|------|--------|-----------|----------|---------|--------|
| 10x  | 84.2%  | 87%       | 0.856    | 15.6ms  | FASTER, slightly better F1 |
| 12x  | 85.5%  | 85%       | 0.854    | 18.6ms  | CURRENT |
| 14x  | 86.8%  | 83%       | 0.852    | 21.6ms  | higher recall, worse F1 |

**Hypothesis**: 10x might be preferred for user experience (26% faster, negligible F1 loss)
**Effort**: 5 minutes (change, test, benchmark)
**Expected ROI**: +26% speed for -0.002 F1 (likely within noise)

### 2. **Reranker Integration** (MEDIUM EFFORT)
Reranker available but disabled by default
- Model: ms-marco-MiniLM-L-6-v2 (cross-encoder)
- Action: Reranks top-15 RRF results
- Estimated precision boost: +5-10%
- Estimated recall impact: Neutral or slight positive
- Estimated F1 gain: +3-5%

**Implementation**:
1. Enable reranker in config.toml
2. Run benchmark
3. Measure precision@10, recall@100, F1

**Expected Outcome**: 85.5% recall, 90%+ precision, F1 > 0.87
**Effort**: 30 minutes (config change, test, analysis)
**Risk**: Could degrade recall if cross-encoder reranks relevant docs down

### 3. **Query Expansion Testing** (MEDIUM EFFORT)
Query expansion available but disabled by default
- Model: tinyllama (ONNX backend)
- Action: Expands queries → runs multiple searches → merges
- Estimated recall boost: +2-3% (short queries benefit more)
- Estimated precision: Neutral or slight loss
- Estimated F1 gain: +1-2%
- Cost: 3x latency multiplier

**Implementation**:
1. Enable query_expansion in config.toml
2. Run benchmark
3. Measure recall, precision, latency

**Expected Outcome**: 87-88% recall, ~84-86% precision, F1 ~0.86
**Effort**: 30 minutes (config change, test, analysis)
**Risk**: Significant latency increase (56ms vs 18.6ms), may not be acceptable

### 4. **Per-Query Adaptive Multiplier** (HIGH-VALUE, DEFERRED)
Dynamic fetch_limit based on query classification
- Common queries: 8x (fast)
- Standard: 12x (balanced)
- Rare/Typo: 20x (comprehensive)

**Benefits**:
- Maintain 85%+ average recall
- Reduce average latency 30-50%
- Better user experience for common case

**Implementation Complexity**: Medium (requires classifier)
**Expected Outcome**: 85% avg recall, 15ms avg latency (+2% vs 12x single config)
**Effort**: 3-4 hours (classifier, routing, benchmarking)
**Status**: DEFERRED (architecture needed first)

## Session 6 Experiment Strategy

### Phase 1: Quick Wins (Fetch Tuning) - 15 min
1. Test 10x vs 12x (confirm 10x slightly better F1)
2. Log results
3. Decide: stay with 12x (conservative) or switch to 10x (aggressive)

### Phase 2: Reranker Testing - 30 min
1. Enable reranker in config
2. Run benchmark with reranker
3. Measure precision@10, recall@100, F1
4. Log results
5. Decision: keep if precision improves

### Phase 3: Query Expansion Testing - 30 min
1. Enable query_expansion in config
2. Run benchmark
3. Measure recall, precision, latency
4. Log results
5. Decision: keep only if latency acceptable

### Phase 4: Analysis & Recommendations - 15 min
1. Summarize findings
2. Recommend final configuration
3. Document tradeoffs

## Expected Session 6 Outcomes

**Scenario A: Reranker Works Well**
- Precision improves to 90%+
- F1 score > 0.87
- Keep reranker enabled
- Minor latency increase acceptable

**Scenario B: Query Expansion Helps Recall**
- Recall improves to 87%+
- F1 score > 0.86
- Latency cost acceptable for recall gain
- Enable for high-recall use cases

**Scenario C: Neither Helps Significantly**
- Reranker doesn't improve precision beyond 85%
- Query expansion brings marginal gains
- Stick with current 12x configuration
- Document why we're at plateau

## Conservative Path (Recommended)

If experiments don't yield significant gains:
1. Keep 12x fetch as production configuration
2. Document that realistic benchmark ceiling ~97% is approaching
3. Note that 85.5% recall + 85% precision is already excellent
4. Recommend per-query routing for future gains
5. Acknowledge that further major improvements require:
   - Real-world validation (labeled dataset)
   - Architectural changes (reranking, query expansion)
   - User feedback loops

## Key Metrics to Track

| Metric | Baseline (12x) | Target | Status |
|--------|---|---|---|
| Recall@100 | 85.5% | 86-87% | TBD |
| Precision@10 | 85% | 88%+ | TBD |
| F1-Score | 0.854 | 0.86+ | TBD |
| Latency/q | 18.6ms | <20ms | TBD |
| Throughput | 53.8 qps | >50 qps | TBD |

## Files to Create/Modify

1. `autoresearch_reranker.sh` - Benchmark reranker impact
2. `autoresearch_query_expansion.sh` - Benchmark query expansion
3. `AUTORESEARCH_SESSION6_RESULTS.md` - Final findings
4. Update `autoresearch.ideas.md` - Prune completed ideas

## Success Criteria

- Session 6 is "successful" if ANY of:
  1. Find configuration with F1 > 0.86 (improvement)
  2. Confirm 85.5% is near-optimal (saturation understanding)
  3. Identify promising direction for future (e.g., per-query routing)
  4. Document clear tradeoffs (speed vs recall)

## Abort Criteria

- Stop early if:
  1. Reranker causes recall to drop >2% (negative impact)
  2. Query expansion adds >50ms latency for <1% recall gain
  3. Changes cause benchmark crashes or data loss
