# Autoresearch: Quality Score Optimization via Prompt Engineering

## Objective

Improve the average quality score of memories across all 5 memory types (Episodic, Semantic, Procedural, Conceptual, Contextual) by optimizing the tinyllama prompts used in quality assessment scoring and potentially enabling grammar-guided (GBNF) output parsing.

The quality score in voidm (0.0-1.0) is computed from:
- Genericity (0.20): Language reuse vs personal context
- Abstraction (0.20): Principle/pattern vs specific instance
- Temporal independence (0.25): No temporal markers like "today"
- Task independence (0.15): Not tied to TODOs/tasks
- Content substance (0.20): Word count (50+ preferred)
- Entity specificity (0.05): Named entity density (10-30% optimal)
- Anti-pattern penalties: Task language excluded for procedural/conceptual

## Metrics
- **Primary**: `avg_quality_score` (unitless, 0.0-1.0, higher is better)
- **Secondary**: `pass_rate` (quality tests passing), `coverage` (memory types tested)

## How to Run
`./autoresearch.sh` — outputs `METRIC avg_quality_score=X.XX` and optional secondary metrics.

## Files in Scope
- `crates/voidm-core/src/quality.rs` — Quality scoring algorithm
- `crates/voidm-core/src/query_expansion.rs` — Prompt templates and grammar definitions
- `crates/voidm-core/src/bin/quality_assessment.rs` — Quality assessment tool/benchmark (if exists)
- `Cargo.toml` — Dependencies (may need to add GBNF support if using grammar-guided output)

## Off Limits
- **voidm data/database**: Do NOT modify test databases or fixtures
- **Test data**: Do NOT add new memory types or change test structure
- **API/Output format**: Do NOT break the public interface of `compute_quality_score()`
- **Existing tests**: Must continue to pass

## Constraints
- **Tests must pass**: `cargo test --lib quality` must show all 13 tests passing
- **No new hard dependencies**: Can use feature flags for optional deps (e.g., GBNF parsing)
- **No overfitting**: Improvements should generalize across memory types, not just pass existing tests
- **Single memory type**: Don't break per-type logic (Procedural/Conceptual handle "done" differently)

## What's Been Tried
- Initial baseline: Quality scoring with current weights and detection patterns
- Prompt templates in query_expansion.rs: FEW_SHOT_STRUCTURED, FEW_SHOT_IMPROVED, FEW_SHOT_INTENT_AWARE (not used for quality scoring yet)

## Ideas to Explore
1. **Prompt-based quality guidance**: Use tinyllama with prompts to re-score or clarify ambiguous memories
2. **GBNF grammar-guided output**: Structure quality feedback to match defined schema
3. **Per-type prompt templates**: Different prompts for episodic vs semantic vs procedural
4. **Weighting adjustments**: Refine the hardcoded weights (0.20, 0.25, etc.) via prompt feedback
5. **Pattern refinement**: Expand entity density detection, temporal marker detection
6. **Substance threshold tuning**: Optimize word count thresholds (15, 50, 300)
