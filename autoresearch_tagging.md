# Autoresearch: Optimize Tinyllama Prompts for Auto-Tagging

## Objective

Replace the current rule-based + NER-based auto-tagger with tinyllama-1.1B LLM prompts that generate more accurate, diverse, and contextually appropriate tags for memory content.

**Scope**: Tinyllama prompts for tag generation only. No changes to NER module, no model architecture changes, no new dependencies.

## Current Implementation (Baseline)

The existing `auto_tagger.rs` uses three strategies:
1. **NER** (Named Entity Recognition) - BERT-based entity extraction
2. **TF** (Term Frequency) - Keyword extraction from content
3. **Rules** - Type-specific patterns for different memory types

**Current behavior**:
- Extracts entity tags from NER (high confidence threshold)
- Extracts top 8 keywords by frequency (excluding stopwords)
- Applies type-specific rules (action verbs, tech keywords, resource types)
- Merges results, deduplicates, removes substrings, limits to 20 tags

**Limitations**:
- Rule-based approach doesn't understand semantic relationships
- TF approach biased toward frequency (not relevance)
- NER requires separate model, adds complexity
- Limited to predefined keyword lists

## Goal: Tinyllama-Based Alternative

Use tinyllama-1.1B to generate tags by:
1. **Understanding context** - Semantic comprehension of content
2. **Diverse tagging** - Generate varied, relevant tags instead of frequency-based
3. **Memory-type awareness** - Generate appropriate tags for episodic/semantic/procedural/conceptual/contextual memories
4. **Domain coverage** - Handle diverse content types without hardcoded rules

## Metrics

### Primary: `tagging_quality_score` (0.0–1.0, higher is better)
- Measures relevance, diversity, and appropriateness of generated tags
- Combines: relevance (40%), diversity (30%), accuracy (20%), memory-type alignment (10%)
- Computed by test harness comparing tinyllama output against baseline + manual validation

### Secondary:
- `latency_ms`: Mean tagging latency (target: <500ms)
- `parse_success_rate`: Percentage of outputs successfully parsed (target: >95%)
- `tag_count_avg`: Average number of tags per memory (target: 5-15 tags)
- `overlap_baseline`: Overlap with existing auto_tagger (target: >60% to ensure compatibility)

## How to Run

```bash
./autoresearch_tagging.sh
```

Outputs `METRIC name=value` format.

## Files in Scope

### Core Implementation
- **`crates/voidm-core/src/auto_tagger_tinyllama.rs`** (NEW)
  - Tinyllama-based tag generation
  - Prompt templates for different memory types
  - Output parsing and validation

### Optional Modifications
- **`crates/voidm-core/src/auto_tagger.rs`**
  - Can add tinyllama as alternative strategy
  - Implement A/B testing or fallback logic
  - Not modified if tinyllama is standalone replacement

### Test/Benchmark
- **`autoresearch_tagging.sh`** (NEW)
  - Quality measurement harness
  - Test set with diverse memory types
  - Scoring logic

## Off Limits

- ❌ Do NOT modify tinyllama inference code
- ❌ Do NOT add new external dependencies
- ❌ Do NOT change the auto_tagger public API
- ❌ Do NOT modify NER module (keep as alternative)
- ❌ Do NOT hardcode memory-specific tags

## Constraints

- ✅ All 104 existing lib tests must pass
- ✅ No new crate dependencies
- ✅ Tagging must remain domain-agnostic
- ✅ Backward compatibility (existing auto_tagger still works)
- ✅ Latency <500ms for tinyllama
- ✅ Parse success rate >95%

## Success Criteria

- ✅ Primary metric (tagging_quality_score) improves from baseline
- ✅ All 104 lib tests pass throughout
- ✅ Latency remains <500ms
- ✅ Parse success rate >95%
- ✅ >60% overlap with baseline (proves it's not random)
- ✅ Works on diverse memory types (episodic, semantic, procedural, conceptual, contextual)

## Implementation Notes

### Measuring Tagging Quality

Quality scoring computed in `autoresearch_tagging.sh` by:
1. Running tinyllama on test memories (50+ diverse memories across all types)
2. For each memory, score on:
   - **Relevance** (40%): Do tags match content? (manual baseline or keyword-based validation)
   - **Diversity** (30%): Do tags span different aspects of content?
   - **Accuracy** (20%): No hallucinations? Valid tags for the content?
   - **Memory-type alignment** (10%): Tags appropriate for memory type?
3. Computing weighted average quality score (0.0–1.0)

### Tinyllama Prompt Design

Template structure for different memory types:

**Episodic** (events, experiences):
```
Generate tags for this memory about an event or experience:
"{content}"

Provide tags that capture:
- Who was involved (people, entities)
- What happened (events, actions)
- When (dates, times)
- Where (locations)
- Why/How (context, relationships)

Tags (CSV format): {tags}
```

**Semantic** (knowledge, definitions):
```
Generate tags for this knowledge/definition:
"{content}"

Provide tags that capture:
- Concepts defined
- Related domains
- Key relationships
- Fundamental properties

Tags (CSV format): {tags}
```

## Session Tracking

- **Session**: autoresearch/tinyllama-auto-tagging-20260319
- **Date**: 2026-03-19
- **Baseline**: TBD (need to establish from current auto_tagger)
- **Status**: ⏳ IN PROGRESS
