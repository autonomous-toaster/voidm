# Autoresearch Ideas Backlog

## Prompt-Based Quality Improvements

### 1. Tinyllama Multi-Prompt Strategy
- Generate per-memory-type prompts for better quality detection
- Use GBNF to enforce structured score output
- Validate scores against current algorithm to catch divergence

### 2. Grammar-Guided Prompt Templates (GBNF)
- Define strict GBNF rules for quality score output format
- Use tinyllama with grammar constraint to reduce hallucination
- Example: Score as structured JSON or CSV for easier parsing

### 3. Per-Type Context Enhancement
- **Episodic**: Include date/time relevance scoring
- **Semantic**: Boost abstraction detection for principles
- **Procedural**: Better "done"/"completed" context awareness
- **Conceptual**: Detect hierarchical relationships
- **Contextual**: Scope boundary detection

### 4. Weight Refinement via LLM Feedback
- Ask tinyllama to rate each scoring dimension
- Use feedback to adjust weights from [0.20, 0.20, 0.25, 0.15, 0.20, 0.05]
- Validate new weights don't break tests

### 5. Pattern Library Expansion
- Add domain-specific temporal markers (not just "today")
- Expand entity specificity detection
- Add project-specific personal language patterns

### 6. Substance Threshold Optimization
- Current: <15 (0.0), 15-50 (0.3), 50-300 (0.95), >300 (0.3)
- Explore: Different thresholds per memory type
- Goal: Encourage "goldilocks" content length
