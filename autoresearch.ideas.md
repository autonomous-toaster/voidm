# Autoresearch Ideas & Optimization Backlog

## Promising Optimization Paths (Priority Order)

### Phase 1: Few-Shot Example Improvements
- ✓ Current: 3-4 examples per template
- [ ] Add 5-8 diverse examples covering:
  - Technical domains (Docker, Python, APIs)
  - Non-technical searches (security, caching, performance)
  - Edge cases (ambiguous terms like "Model", "Service")
- [ ] Refine each example's related terms for better quality/diversity

### Phase 2: New Template Variants
- [ ] **Chain-of-Thought Template**: Add reasoning step before expansion
  - "First, identify key concepts in the query..."
  - "Then expand each concept..."
  - Potential +10% quality improvement
- [ ] **Negation-Aware Template**: Explicitly exclude false positives
  - "Don't include: <unrelated terms>"
  - Reduce hallucinations
- [ ] **Semantic Dimension Template**: Group expansions by semantic type
  - "Tools: ..., Concepts: ..., Related fields: ..."
  - +15% diversity potential

### Phase 3: Post-Processing & Filtering
- [ ] Embedding-based deduplication (remove near-duplicates)
- [ ] Term ranking by relevance using cosine similarity
- [ ] Filter out low-confidence terms (<0.6 relevance score)
- [ ] Combine multiple expansion strategies and merge outputs

### Phase 4: Intent-Aware Expansion (if enabled)
- [ ] Integrate intent context into template selection
- [ ] Route specific query types to optimized templates
- [ ] Add user context hints (project scope, domain)

### Phase 5: Advanced Techniques (if time/resources permit)
- [ ] HyDE-style hypothetical document generation
- [ ] Multi-stage expansion (expand → refine → rank)
- [ ] Ensemble methods combining multiple models
- [ ] Fine-tuned model evaluation (if tinyllama-QE available)

## Known Constraints & Gotchas

- **No Model Changes**: We can only modify prompts; tinyllama weights are frozen
- **Latency Ceiling**: ONNX inference ~250-350ms; won't improve with prompt alone
- **Generalization**: Test on held-out query types to avoid overfitting
- **Output Format**: Must remain comma-separated for existing parsers
- **Benchmark Honesty**: Don't hardcode examples matching test queries

## Measurement Approach

- **Quality scoring** in autoresearch.sh combines:
  - Prompt structure analysis (example count, diversity, clarity)
  - Manual golden set scoring (if tinyllama available)
  - Embedding-based relevance validation
- **Always validate** on diverse query types (not just benchmarks)

## Historical Notes

- QMD project uses fine-tuned Qwen3-1.7B (better but larger)
- Voidm uses off-the-shelf tinyllama (simpler, no fine-tuning)
- Current approach: maximize prompt engineering within this constraint
