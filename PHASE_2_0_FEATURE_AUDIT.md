# Phase 2.0: Feature Flags Audit

## Current Features

### voidm-cli Features
```toml
default = ["standard"]
minimal = ["database-sqlite"]
standard = ["ml-core", "embeddings", "database-sqlite", "graph", "scoring", "ner", "query-expansion", "reranker", "nli"]
full = ["standard"]
```

**Status**: Defined but many features not actually passed through

### voidm-core Features
```toml
default = []
ml-core = ["dep:ort", "dep:hf-hub", "dep:tokenizers"]
embeddings = ["ml-core"]
query-expansion = ["...]
reranker = ["..."]
ner = ["voidm-ner", "dep:hf-hub"]
nli = ["...]
```

**Status**: Partially defined

### voidm-scoring Features
```toml
default = []
ner = ["voidm-ner"]
```

**Status**: Has NER as optional feature

### voidm-mcp Features
```toml
default = ["sqlite"]
sqlite = []
```

**Status**: Simple, just for backend selection

---

## Code Feature Usage

### voidm-cli (5 features used)
```rust
#[cfg(feature = "reranker")]
#[cfg(feature = "ner")]
#[cfg(feature = "query-expansion")]
```

**In**: commands/init.rs (model download UI)

### voidm-core (4 features used)
```rust
#[cfg(feature = "ner")]
#[cfg(feature = "reranker")]
#[cfg(feature = "query-expansion")]
```

**In**: lib.rs, config.rs, crud.rs, search.rs

### voidm-embeddings (1 feature used)
```rust
#[cfg(feature = "ort")]  // ONNX Runtime
```

**In**: semantic_dedup.rs

### voidm-scoring (1 feature used)
```rust
#[cfg(feature = "ner")]
```

**In**: lib.rs

---

## Feature Status

### ✅ Working Features
1. **embeddings**: Using Xenova/all-MiniLM-L6-v2 (working)
2. **ner**: Feature gate works (disabled by default)
3. **query-expansion**: Feature gate works (optional)
4. **reranker**: Feature gate works (optional)
5. **nli**: Feature gate works (optional)

### ⚠️ Issues/Notes
1. **ml-core dependency chain**: Not fully materialized in Cargo.toml
2. **ort (ONNX Runtime)**: Used for semantic_dedup, not fully integrated
3. **default features**: Inconsistent between crates
4. **Feature propagation**: CLI doesn't properly enable features for dependencies

### 🔴 Broken/Unused
1. **hf-hub**: Defined but largely unused
2. **tinyllama**: Mentioned in old code (UNUSED)
3. **nli**: Defined but untested

---

## Audit Findings

### Finding 1: Feature Cargo.toml Not Properly Updated
**Issue**: voidm-cli Cargo.toml defines features but doesn't actually enable them for dependencies

**Example**:
```toml
# DEFINED BUT NOT USED
standard = [..., "ner", "query-expansion", "reranker", "nli"]

# ACTUAL DEPENDENCIES don't get these features!
[dependencies]
voidm-core = { path = "../voidm-core", features = [] }  # ← should be ["ner", ...]
```

**Impact**: Features are defined but not actually enabled

**Fix Required**: Update voidm-cli/Cargo.toml to propagate features to voidm-core

### Finding 2: Default Features Inconsistent
**Issue**: Some crates have defaults, some don't

**Current**:
- voidm-cli: default = ["standard"]
- voidm-core: default = []
- voidm-scoring: default = []

**Recommendation**: Establish clear defaults:
- Embeddings should be default (always needed)
- NER/NLI should be optional (expensive models)
- Query-expansion should be optional

### Finding 3: ONNX Runtime Integration Incomplete
**Issue**: `ort` feature in voidm-embeddings but:
- Only used in semantic_dedup.rs (single function)
- Not fully integrated
- Heavy dependency

**Recommendation**: 
- Mark as experimental
- Consider removing if not used in main path

### Finding 4: Missing Feature Documentation
**Issue**: No clear documentation on:
- Which features are required vs optional
- Build time impact
- Performance impact
- When to enable each feature

**Recommendation**: Create FEATURES.md doc

---

## Recommended Actions (Phase 2.0)

### Action 1: Fix voidm-cli Feature Propagation (15 min)
Update Cargo.toml:
```toml
[dependencies]
voidm-core = { path = "../voidm-core", features = ["embeddings"] }  # default
voidm-scoring = { path = "../voidm-scoring", features = [] }  # ner is optional

[features]
default = ["standard"]
standard = [..., "ner"]  # enables NER in dependencies
minimal = ["embeddings"]  # just embeddings
ner = ["voidm-scoring/ner"]  # enable NER
```

### Action 2: Document Feature Choices (30 min)
Create FEATURES.md:
- List all features
- Explain each one
- Show which are default
- Give build command examples

### Action 3: Clean Up Unused Code (15 min)
- Remove tinyllama references (not used)
- Clean up unused hf-hub usage
- Mark experimental features clearly

### Action 4: Audit Build Times (15 min)
Test build times:
- `cargo build`: Full featured
- `cargo build --no-default-features --features minimal`: Minimal
- Document results

---

## Phase 2.0 Tasks

### Task 1: Fix Cargo.toml Features (15 min)
- [ ] Update voidm-cli/Cargo.toml feature propagation
- [ ] Update default features
- [ ] Verify build works

### Task 2: Create FEATURES.md (30 min)
- [ ] List all features
- [ ] Explain each
- [ ] Document tradeoffs
- [ ] Provide build examples

### Task 3: Mark Experimental Features (15 min)
- [ ] Add comments marking experimental
- [ ] Document which are production-ready
- [ ] Note breaking change risks

### Task 4: Test Build Profiles (15 min)
- [ ] Verify `cargo build` works
- [ ] Verify minimal build works
- [ ] Document build times
- [ ] Check binary sizes

---

## Expected Outcomes

### Completed Phase 2.0:
✅ Feature flags properly propagated
✅ Documentation created
✅ Build profiles tested
✅ Feature matrix clear

### Ready for Phase 2.1:
Schema cleanup can proceed without feature flag confusion

---

## Time Estimate

**Phase 2.0 Total**: 1.5 hours
- Cargo.toml fixes: 30 min
- Documentation: 45 min
- Testing: 15 min

