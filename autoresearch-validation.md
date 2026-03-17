# Autoresearch: Quality Scoring Validation

## Objective

Validate that the quality scoring system from the previous optimization session actually works correctly on diverse test cases. Ensure:
1. Good memories score > 0.70
2. Bad memories score < 0.50
3. Scoring reflects memory type correctly (Procedural/Conceptual allow "done", others penalize it)
4. No overfitting to specific patterns
5. Detailed feedback on stdout for transparency

## Metrics
- **Primary**: `validation_pass_rate` (0.0-1.0, higher is better)
- **Secondary**: `test_count` (number of diverse test cases validated)

## How to Run
`bash autoresearch-validation.sh` — validates quality scoring on 16+ diverse test cases, outputs detailed feedback

## Test Suite

**Good Memories** (expect > 0.70):
- Generic semantic principle (distributed systems)
- Clear procedural steps (deployment)
- Well-explained concept (circuit breaker)
- Structured episodic event (outage analysis)
- Example-rich contextual memory

**Bad Memories** (expect < 0.50):
- Task logs ("I did", "completed")
- Status updates ("Status: ", "Milestone: ")
- Too generic ("test", "done")
- Personal actions ("I fixed", "we worked")
- Temporal markers ("today", "right now")
- Repetitive content (low diversity)

**Mixed Quality** (expect 0.40-0.70):
- Somewhat generic but useful
- Needs structure but has content

**Memory Type Specific**:
- Procedural: "done" is OK (0.50-0.95)
- Semantic: "done" is penalized (<0.50)

## Files in Scope
- `crates/voidm-core/src/bin/quality_validation.rs` — Validation binary with 16+ test cases
- `autoresearch-validation.sh` — Test runner with detailed stdout feedback
- `crates/voidm-core/src/quality.rs` — Quality scoring being validated (read-only)

## Off Limits
- Do NOT modify `quality.rs` during validation (we're testing, not optimizing)
- Do NOT change test expectations to make them pass
- Do NOT cache or hardcode scores

## Constraints
- All 13 unit tests in `cargo test --lib quality` must still pass
- Validation binary must compile cleanly
- Must provide detailed stdout feedback on each test
- No overfitting: test cases are representative, not hand-crafted to pass

## Success Criteria
- `validation_pass_rate >= 0.95` (at least 15/16 tests correct)
- Zero compiler warnings
- Clear, actionable stdout feedback
- Unit tests still passing
- No cheating: scoring reflects genuine quality differences
