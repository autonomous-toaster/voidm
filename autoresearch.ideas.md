# Autoresearch Ideas Backlog

## Completed Experiments (23 total)

### Pattern Detection & Coverage
1. ✅ Expanded temporal marker detection (added "currently", "recently", date patterns, etc.)
2. ✅ Enhanced abstraction detection (added "i created", "i implemented", "i worked", etc.)
3. ✅ Improved genericity detection (added pronouns like "myself", "yours", "their", project refs)
4. ✅ Expanded status prefix detection (added "session:", "todo:", "fix:", "issue:", "pr:", etc.)
5. ✅ Added instance-specific marker detection (TODO-, session-, date patterns in abstraction)

### Per-Memory-Type Customization
6. ✅ Per-memory-type substance thresholds (Procedural 20+, Conceptual 40+, Episodic 30+, Semantic/Contextual 50+)
7. ✅ Episodic-aware temporal scoring (lighter penalties for episodic, stricter for others)
8. ✅ Per-type entity specificity (Episodic 20-40%, Procedural/Conceptual low ok, Semantic/Contextual 10-30%)
9. ✅ Per-type genericity penalties (Semantic/Conceptual stricter 0.30, Contextual 0.25, Episodic/Procedural 0.20)

### Graduated Penalty Systems
10. ✅ Temporal independence graduated penalties (0→0.95, 1→0.65, 2→0.45, 3→0.25, 4+→0.10)
11. ✅ Task independence graduated penalties (0→0.95, 1→0.75, 2→0.50, 3→0.30)

### Quality Bonuses & Incentives
12. ✅ Actionable pattern bonus (detects "when", "if", "always", "never", "use", "avoid", "ensure")
13. ✅ Structured format detection (lists, key-value, arrows, multiple paragraphs)
14. ✅ Citation bonus (detects URLs, RFC, GitHub references)
15. ✅ Cross-referential bonus (concept:, tag:, related:, see also, similar to, contrast with, extends)
16. ✅ Knowledge markers bonus (important, key insight, best practice, pattern:, principle:, lesson, tradeoff)
17. ✅ Generic/template content penalty (heavily penalize single words like "todo", "done", "test")

### Weight Optimization
18. ✅ Conservative weight adjustments: genericity 0.15→0.14, abstraction 0.15→0.14, temporal 0.35→0.36, entity 0.05→0.06

## Future Ideas (Not Pursued)

### Advanced Pattern Detection
- Keyword density analysis per memory type
- Multi-word n-gram patterns (e.g., "was working on")
- Sentiment analysis to detect emotional language
- Abbreviation/acronym consistency

### Machine Learning Approaches
- Use tinyllama with GBNF to generate quality scores
- Train a lightweight linear model on real memory data
- Use LLM-generated labels as ground truth

### Structural Analysis
- Parse markdown structure and reward well-formatted content
- Detect code blocks and reward technical examples
- Validate hyperlink persistence

### Integration with Existing Features
- Use graph relationships to contextualize memory quality
- Cross-validate quality scores with retrieval metrics
- Link quality trends to concept evolution

## Constraints & Success Criteria

**Must Pass**: All 13 quality unit tests
**Must Not Break**: Public API of `compute_quality_score()`
**No New Hard Dependencies**: Feature gates ok
**No Overfitting**: Improvements must generalize

## Current Session (Resume #2)

### Analysis of Remaining Failures (2/15)
1. **Mixed - OK Quality** (0.887, expected 0.40-0.75)
   - Content: Docker, containers, volumes, caching (good, actionable, technical)
   - Assessment: System correctly identifies as high-quality
   - Verdict: Test expectation might be unrealistic

2. **Mixed - Needs Work** (0.765, expected 0.30-0.65)
   - Content: "I learned about REST APIs today..."
   - Has: personal language, temporal marker, but also substance
   - Assessment: Score is reasonable for marginal mixed-quality content
   - Verdict: Test expectation might be strict, score is defensible

### Ideas for Future Improvements (non-overfitting)

#### High Value (should explore)
- [ ] Compare avg_quality_score across broader memory corpus
- [ ] Analyze other memory types beyond validation test cases
- [ ] Look for patterns in what scores well vs poorly across real data
- [ ] Consider context/relationship-based quality validation

#### Medium Value (optional)
- [ ] Use GGUF validation tool to analyze real failure modes
- [ ] Implement sentiment analysis for confidence markers
- [ ] Improve code block detection and reward for technical content
- [ ] Better markdown structure parsing

#### Low Value (likely overfitting)
- [ ] ~~Substance floor (breaks unit tests)~~
- [ ] ~~Aggressive penalty tuning for short content~~
- [ ] ~~Single-test-case pattern additions~~

### Recommendation
Current state (86% validation, 13/13 unit tests) appears to be a good local optimum. Further tuning of the 2 failing cases risks overfitting. Focus should shift to:
1. Validating generalization on real memory data
2. Exploring other quality dimensions beyond current 6
3. Cross-validation with user feedback or retrieval metrics
