# voidm v5 Remediation Plan

**Status**: Architecture misaligned with vision. 13 critical gaps identified + 1 NEW: 174 sqlx violations.
**KEY INSIGHT**: Generic node/edge format (like filemind) is the true foundation.
**CRITICAL CONSTRAINT**: Local config file during entire 20+ day implementation cycle to protect production.

---

## CRITICAL DISCOVERY: SQLx Violations Audit (Phase 1 Redefined)

**Severity**: BLOCKS all other work. Architecture violation preventing backend agnostic design.

**Finding**: 174 sqlx references scattered across 23 files outside backend implementations:

| Crate | Violations | Status |
|-------|-----------|--------|
| voidm-core | 65 | CRITICAL |
| voidm-graph | 26 | CRITICAL |
| voidm-cli | 27 | HIGH |
| voidm-tagging | 8 | HIGH |
| voidm-ner | 2 | HIGH |
| voidm-mcp | 1 | MEDIUM |
| **VIOLATIONS** | **174** | **MUST FIX** |
| voidm-sqlite | 35 | ALLOWED |
| voidm-postgres | 106 | ALLOWED |

**Root Cause**: DB trait exists but not used. All code bypasses trait and uses sqlx directly.

**Impact**:
- Neo4j backend impossible to implement
- Backend switching broken
- Architecture claim "multi-backend support" false

**Solution**: See Phase 1 below (completely redesigned based on this audit)

---

## CRITICAL DISCOVERY: Chunk Ordering Issue (Phase 0 Addition)

**Severity**: Blocks proper chunk reconstruction. Schema incomplete.

**Finding**: Chunks have no ordering mechanism (sequence_num, char positions).

**Current Problem**:
```
Memory: "Word1 Word2 Word3 Word4 Word5"

Chunks created (with 50% overlap):
  chunk_abc: "Word1 Word2 Word3"
  chunk_def: "Word2 Word3 Word4 Word5"
  
Stored with random UUIDs (no sequence):
  memory_chunks { id: chunk_abc, content: "...", memory_id: mem_123 }
  memory_chunks { id: chunk_def, content: "...", memory_id: mem_123 }

If reconstruct in DB order (random):
  Result: "Word2 Word3 Word4 Word5" + "Word1 Word2 Word3" 
  OR: "Word1 Word2 Word3" + "Word2 Word3 Word4 Word5"
  
Expected: "Word1 Word2 Word3 Word4 Word5" (with no duplication)
```

**What's Missing**:
- `sequence_num`: Position in memory (0, 1, 2, ...)
- `char_start`: Byte offset in original (0, 8, 16, ...)
- `char_end`: Byte offset in original (24, 40, ...)

**Impact**:
- ✗ Cannot reconstruct full memory from chunks
- ✗ Cannot de-duplicate overlapped content
- ✗ Cannot verify chunk consistency
- ✗ Cannot update memory correctly (which chunks to delete?)
- ✗ Cannot provide chunk-based entity linking with context
- ✗ Cannot show search excerpts with surrounding context

**Solution**: Add to Phase 0 (3 fields, low cost):
```
NodeType::Chunk:
  sequence_num: i32       (0, 1, 2, ...)
  char_start: i32        (byte offset)
  char_end: i32          (byte offset)
  
Memory -[:HAS_CHUNK]-> Chunk:
  properties: { sequence_num: 0 }  (enables ordered traversal)
```

---

## Phase -1: Config & Backend Override System (BLOCKING ALL) ⚠️ MUST BE FIRST
**Goal**: Enable safe local development config + flexible three-stage priority chain

**Why This is CRITICAL FIRST**: Implementation cycle (20+ days) will modify:
- Config loading logic (Phase -1)
- Database schema (Phase 0)
- All SQL operations (Phase 1-8)
- All CLI commands (Phases 1-8)

**Risk Without Local Config**: Accidental changes affect production database or config

**Solution**: 
1. Local `.voidm.dev.toml` (auto-detected, not in git)
2. Three-stage priority chain (config location → backend selection → connection params)
3. Smart detection: uses local config when in project, system config when outside

**What This Enables**:
- Single local config file for entire dev cycle (20+ days)
- Automatically selected when in project directory
- Production config never touched
- Default backend during dev: `dev` (safe)
- Can still access production: `--config ~/.config/voidm/config.toml --backend prod`

---

### Local Config Auto-Detection (NEW)

**Priority Chain for Config File Location**:
```
HIGHEST PRIORITY:
1. --config /path/to/config.toml         (explicit CLI override)
2. VOIDM_CONFIG=/path/to/config.toml     (env override)
3. ./.voidm.dev.toml                     (local auto-detected if in project)
4. ./.voidm.local.toml                   (alternative local config)
5. ./config.local.toml                   (fallback local config)
6. XDG_CONFIG_HOME/voidm/config.toml     (XDG standard)
7. ~/.config/voidm/config.toml           (Linux home)
8. ~/Library/Preferences/voidm/config.toml (macOS home)
LOWEST PRIORITY: Hardcoded defaults
```

**Smart Detection**:
```rust
fn is_dev_environment() -> bool {
    // True only if we're in the project directory
    std::path::Path::new("Cargo.toml").exists() ||
    std::path::Path::new(".git").exists()
}
```

---

### Local Development Config: `.voidm.dev.toml`

**Create in project root** (NOT committed to git):

```toml
# ~/.voidm/.voidm.dev.toml (project root)
# Added to .gitignore - never committed
# Auto-detected and used during entire implementation cycle

[backends]
default = "dev"  # ← IMPORTANT: Dev backend by default during dev

[backends.prod]
type = "sqlite"
path = "~/.local/share/voidm/memories.db"

[backends.dev]
type = "sqlite"
path = "~/.local/share/voidm/memories-phase0.db"

[backends.staging]
type = "sqlite"
path = "~/.local/share/voidm/memories-staging.db"

[backends.test]
type = "sqlite"
path = "/tmp/voidm-test.db"

# Copy settings from production config
[embeddings]
enabled = true
model = "Xenova/all-MiniLM-L6-v2"

[search]
mode = "hybrid-rrf"
default_limit = 10
```

**Gitignore** (in project):
```
.voidm.dev.toml
.voidm.local.toml
config.local.toml
config.*.toml
```

---

### Usage During Implementation Cycle (20+ days)

**Day 1: Setup**:
```bash
cd ~/.voidm
# Create local config (auto-detected from now on)
cat > .voidm.dev.toml << 'EOF'
[backends]
default = "dev"
[backends.prod]
type = "sqlite"
path = "~/.local/share/voidm/memories.db"
[backends.dev]
type = "sqlite"
path = "~/.local/share/voidm/memories-phase0.db"
EOF

# Verify it's detected
voidm info  # Shows: Using config: .voidm.dev.toml, backend: dev
```

**Days 1-20: Development**:
```bash
# Default: automatically uses local config, dev backend
voidm search "query"        # Dev backend only
voidm add -t episodic -c "Phase 0 progress: ..."
voidm export                # Exports from dev database

# All modifications isolated to dev backend
cargo build && cargo test
voidm search "testing notes"  # All dev database
```

**When Production Access Needed**:
```bash
# Explicitly access production config + backend
voidm search --config ~/.config/voidm/config.toml --backend prod
voidm export --config ~/.config/voidm/config.toml --backend prod > backup.jsonl

# Or use environment variable
VOIDM_CONFIG=~/.config/voidm/config.toml VOIDM_BACKEND=prod voidm search "query"
```

**Day 21: Final Testing Before Cutover**:
```bash
# Export prod data
voidm export --config ~/.config/voidm/config.toml --backend prod > migration.jsonl

# Import to dev for testing with prod data
voidm import migration.jsonl

# Verify all phases work with real data
voidm search "query"  # Uses dev backend with prod data

# When ready: update prod config, switch backend, restart
```

---

### Code Changes

**1. Config Struct Extension** (voidm-core/src/config.rs):
- Add `pub fn load_from(path: Option<String>) -> Self`
- Add `is_dev_environment()` - checks for Cargo.toml or .git
- Add `find_local_config()` - looks for .voidm.dev.toml, etc.
- Add `resolve_config_path()` - implements full priority chain (9 levels)
- Maintain backward compat: auto-create [backends.prod] from [database]

**2. Two-Stage CLI Parsing** (voidm-cli/src/main.rs):
- Stage 1: Early parse for `--config` / `VOIDM_CONFIG`
- Resolve config file path (now includes local detection)
- Load config from resolved path
- Stage 2: Full CLI parse with `--backend` / `VOIDM_BACKEND`
- Apply backend selection from config

**3. CLI Info Command**:
- Add output showing which config file is loaded
- Display detected backend
- Show all available backends from config

**4. Backward Compatibility**:
- Old config [database.sqlite_path] still works
- Users outside project directory: uses system config (unchanged)
- Existing users unaffected
- No breaking changes

**Effort**: 4.5-5.5 hours
- Config struct + load_from + local detection: 2 hours
- Two-stage CLI parsing: 1.5 hours
- Backend resolution + info command: 1 hour
- Backward compat + tests: 1 hour

**Status**: Not started
**Blocker**: Everything else depends on this

**Critical Success Criteria**: 
- ✓ `.voidm.dev.toml` in project root auto-detected when in project
- ✓ Default backend during dev: `dev` (safe, not `prod`)
- ✓ Production config never touched (~/.config/voidm/config.toml safe)
- ✓ `voidm info` shows which config file is being used
- ✓ Can override: `VOIDM_CONFIG=~/.config/voidm/config.toml voidm search "test"`
- ✓ Local config files added to .gitignore, never committed
- ✓ Users outside project directory unaffected (uses system config)
- ✓ All legacy scenarios still work (backward compat)

---

## Phase 0: Generic Node/Edge Format for SQLite (BLOCKING ALL)
**Goal**: Implement filemind-style generic node/edge abstraction in voidm-sqlite

**CRITICAL ADDITION**: Chunks must have orderable IDs for reconstruction consistency

**Why After Phase -1**: Phase -1 enables safe Phase 0 development:
- Local `.voidm.dev.toml` created
- Default backend: `dev`
- Production database untouched
- Safe to experiment with Phase 0 schema changes

**Pattern** (based on filemind):

```rust
pub enum NodeType { Memory, Chunk, Tag, MemoryType, Scope }
pub enum EdgeType { ChunkOf, TaggedWith, HasType, InScope }

trait Database {
    async fn create_node(
        &self,
        node_type: NodeType,
        id: &str,
        properties: HashMap<String, Value>,  // ← GENERIC JSON PROPERTIES
    ) -> Result<()>;
    
    async fn create_edge(
        &self,
        from_id: &str,
        edge_type: EdgeType,
        to_id: &str,
        properties: Option<HashMap<String, Value>>,
    ) -> Result<()>;
}
```

**SQLite Schema** (generic):
```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,           -- Memory, Chunk, Tag, MemoryType, Scope
    properties TEXT NOT NULL,     -- JSON object with all fields
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    INDEX(type)
);

CREATE TABLE edges (
    id TEXT PRIMARY KEY,
    from_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,      -- ChunkOf, TaggedWith, HasType, InScope
    to_id TEXT NOT NULL,
    properties TEXT OPTIONAL,     -- JSON for edge metadata (includes sequence info)
    created_at TIMESTAMP,
    UNIQUE(from_id, edge_type, to_id),
    FOREIGN KEY(from_id) REFERENCES nodes(id),
    FOREIGN KEY(to_id) REFERENCES nodes(id)
);
```

**IMPORTANT: Embedding Storage is Backend-Specific**
- NOT stored in generic nodes table (no BLOB column)
- Each backend optimizes embeddings:
  - **SQLite**: `sqlite-vector` native type (table: node_embeddings)
  - **Neo4j**: `APOC` vector procedures + indexes
  - **PostgreSQL**: `pgvector` extension (table: node_embeddings)
- DB trait abstracts: `store_chunk_embedding()`, `store_memory_title_embedding()`, `search_by_embedding()`, `search_by_title_embedding()`
- Each backend implements with backend-native optimizations (+30-70% perf)

**Title Embeddings** (NEW):
- Memory node: Generate 384-dim embedding for title during creation
- Store in backend-specific embeddings table
- Regenerate on memory update (if title changes)
- Use case: Title-based semantic search + title-content drift detection

**Edge Weights** (NEW):
- All edges have `weight: f32` property (default 1.0)
- Stored in edges.properties: `{ weight: 0.95, auto_generated: true, ... }`
- Use cases:
  - Entity mention count: Chunk -[:MENTIONS]-> Entity with weight = count
  - Tag confidence: Memory -[:TAGGED_WITH]-> Tag with weight = 1.0 (user) or 0.7 (auto)
  - Relationship strength: Any edge can have numeric weight for ranking
- Query: Support weighted filtering and ranking

**CRITICAL: Chunk Node Structure** (NEW DISCOVERY)
```
NodeType::Chunk MUST include ordering fields:
{
  id: "chunk:uuid",
  memory_id: "memory:uuid",
  sequence_num: 0,              // ← NEW: Position (0, 1, 2, ...)
  char_start: 0,                // ← NEW: Byte offset in original
  char_end: 2048,               // ← NEW: Byte offset in original
  content: "chunk text...",
  embedding_dim: 384,
}

Edge: Memory -[:HAS_CHUNK]-> Chunk
properties: { sequence_num: 0 }  // Enables ordered traversal
```

**Why Chunk Ordering is CRITICAL**:
1. **Reconstruction on retrieval**: GET /memory/{id} should reconstruct from chunks
2. **Overlap de-duplication**: Chunks overlap by 50 tokens; need to know which text to skip
3. **Update consistency**: When memory content updated, need to know which chunks to delete
4. **Entity linking context**: NER on chunk needs surrounding chunks for context
5. **Corruption detection**: Verify chunks reconstruct to original content

**Problem Without Ordering**:
- Chunk 1: "Word1 Word2 Word3"
- Chunk 2: "Word2 Word3 Word4 Word5" (50% overlap)
- Without ordering: Reconstruct might duplicate as "Word1 Word2 Word3 Word2 Word3 Word4 Word5"
- With ordering: Can detect overlap via char_start/char_end and de-duplicate

**Migration Path**:
- Current `memories` table → `NodeType::Memory` rows with properties JSON
- Current `chunks` table → `NodeType::Chunk` rows with sequence_num, char_start, char_end
- Current `links` table → generic `edges` table with `edge_type` = rel name
- Current `memories_chunks` → `edges` with `edge_type = HAS_CHUNK`, properties include sequence

**Effort**: 3-4 days
- Schema design + chunk ordering: 1.5 days
- Generic CRUD operations: 1 day
- Cypher translation: 1 day
- Tests + verification: 1 day

**Outcome**: voidm-sqlite now generic. Can add any node type without schema changes. Backend truly abstracted.

---

## Phase 1: Backend Abstraction (CRITICAL - BLOCKING ALL OTHER PHASES)
**Goal**: Remove 174 sqlx violations, route all through DB trait for true backend agnostic architecture

**Audit Finding**: 174 sqlx references spread across 23 files outside voidm-sqlite/postgres:
- voidm-core (65 violations): crud.rs (51), migrate.rs (11), models/search/vector (3)
- voidm-graph (26 violations): traverse.rs (13), ops.rs (9), cypher/mod.rs (4)
- voidm-cli (27 violations): stats (10), graph (7), other commands (10)
- voidm-tagging (8), voidm-ner (2), voidm-mcp (1)
- Only voidm-sqlite (35) and voidm-postgres (106) should have sqlx

### 1.1 Extend voidm-db-trait (2-3 hours)
Add missing trait methods needed by all implementations:
- `get_statistics()` → Statistics struct (memory_count, tag_count, edge_count, etc.)
- `batch_insert_memories()` → for efficient bulk operations
- `begin_transaction()` → for atomic multi-step operations
- Update all existing methods to handle new node types (Entity, EntityType)

### 1.2 Fix voidm-core CRITICAL (1 day)
**crud.rs (51 violations)**:
- Replace `sqlx::SqlitePool` → `&dyn Database` in all function signatures
- add_memory, get_memory, delete_memory, list_memories, update_memory
- link_memories, unlink_memories, resolve_memory_id
- tag operations: all now use Database trait

**migrate.rs (11 violations)**:
- Move migration logic OUT of voidm-core
- Create trait method: `ensure_schema()` (already exists)
- Backend implementations handle their own migrations

**models.rs, search.rs, vector.rs (4 violations)**:
- Remove sqlx imports, use traits only

### 1.3 Fix voidm-graph CRITICAL (12-16 hours)
**traverse.rs (13 violations)**:
- Replace direct sqlx calls with `db.get_neighbors(id, depth)` trait method
- BFS/DFS traversal now delegates to backend
- Schema assumptions removed

**ops.rs (9 violations)**:
- Graph operations use `db.query_cypher()` for execution
- Return results in generic format (no SQL schema assumed)

**cypher/mod.rs (4 violations)**:
- Wrap Cypher execution in trait method
- No direct sqlx calls

### 1.4 Fix voidm-cli (8-10 hours)
**stats.rs (10 violations)**:
- All statistics queries use new `db.get_statistics()` trait method
- Or use new `count_nodes(type)` / `count_edges(type)` methods

**graph.rs (7 violations)**:
- Use `db.get_neighbors()` instead of direct queries
- Use `db.query_cypher()` for graph analysis

**Other commands (10 violations)**:
- add.rs, delete.rs, get.rs, etc. all use trait methods
- No direct database access from CLI layer

### 1.5 Fix voidm-tagging, voidm-ner, voidm-mcp (3-4 hours)
- voidm-tagging (8 violations): use `db.create_tag()`, `db.link_tag_to_memory()`
- voidm-ner (2 violations): use `db.get_or_create_entity()` trait method
- voidm-mcp (1 violation): use trait for MCP operations

### 1.6 Implement voidm-db-trait in backends (8-10 hours)
**voidm-sqlite**:
- Add implementations of new trait methods (statistics, batch, transactions)
- Ensure all 35 sqlx references properly implement trait surface
- SQLite-specific optimizations remain isolated here

**voidm-postgres**:
- Mirror voidm-sqlite implementations
- PostgreSQL-specific features isolated here
- Ensure 106 sqlx references properly implement trait surface

### 1.7 Validation & Testing (2-3 hours)
- Zero sqlx imports outside voidm-sqlite/postgres/postgres-tests
- All CRUD operations use Database trait
- All graph operations use Database trait
- All CLI commands use Database trait
- All tests pass with SQLite backend
- Neo4j backend now implementable (verify schema)

**Estimated Effort**: 2.5-3 days (aggressive) or 3-4 days (comfortable)

**Outcome**: 
- ✓ Zero sqlx violations in core/graph/cli/tagging/ner/mcp
- ✓ All operations route through Database trait
- ✓ Neo4j backend implementation now possible
- ✓ True backend-agnostic architecture
- ✓ No SQL-specific assumptions in non-backend code

---

## Phase 2: Dead Code Removal (HIGH PRIORITY)
**Goal**: Purge Concept system entirely

- **2.1** Remove all Concept-related code
  - Delete from export.rs: ExportRecord::Concept, ConceptRecord
  - Delete from neo4j backend: Concept constraints, node creation
  - Estimated: 4 hours
  
- **2.2** Clean up ontology edges
  - Verify IsA/InstanceOf/HasProperty edges are remapped
  - Document what replaces concept relationships
  - Estimated: 2 hours

**Outcome**: Code no longer references concepts. Schema clean.

---

## Phase 3: First-Class Citizens (DESIGN)
**Goal**: Make MemoryType and Scope real graph nodes, linked from add_memory()

**IMPORTANT: Type/Scope Always User-Provided (NOT Automatic)**
- User provides via CLI: `voidm add "..." --type Episodic --scope project/auth`
- If not provided: Memory remains untyped/unscoped
- On-demand: MemoryType/Scope nodes created first time that value used
- No automatic inference from content

- **3.1** Create MemoryType nodes in generic format
  - NodeType::MemoryType with values: Episodic, Semantic, Procedural, Conceptual, Contextual
  - Stored as generic nodes with properties (on-demand creation)
  - Neo4j: Same pattern with MemoryType:Label nodes
  - Estimated: 1.5 hours

- **3.2** Create Scope nodes in generic format
  - NodeType::Scope with scope name as property
  - Stored as generic nodes (on-demand creation)
  - Neo4j: Same pattern with Scope:Label nodes
  - Estimated: 1.5 hours

- **3.3** Create edge types for type/scope relationships
  - EdgeType::HasType (Memory -[:HAS_TYPE]-> MemoryType)
  - EdgeType::InScope (Memory -[:IN_SCOPE]-> Scope)
  - Estimated: 1 hour

- **3.4** DB trait: Add methods for type/scope queries and linking
  - get_memories_by_type(memory_type), get_memories_by_scope(scope)
  - link_memory_to_type(memory_id, type_name), link_memory_to_scope(memory_id, scope_name)
  - Estimated: 2 hours

- **3.5** Wire add_memory() to accept user-provided type/scope
  - add_memory() signature: accept `memory_type: Option<String>`, `scope: Option<String>`
  - If type provided: create Memory -[:HAS_TYPE]-> MemoryType edge
  - If scope provided: create Memory -[:IN_SCOPE]-> Scope edge
  - Estimated: 1 hour

**Outcome**: Query by type/scope. Graph fully structured. User-controlled typing.

---

## Phase 3.5: Entity Graph (NER - Named Entity Recognition)

**Goal**: Enable NER feature to extract named entities and create Entity nodes + relationships

**Separate from Tags**: 
- **NER** extracts NAMED entities (people, organizations, locations, misc)
- **Auto-tagging** generates TOPIC tags (what memory is about)
- Completely independent features, both optional

**Data Model**:

```
NodeType::Entity {
  id: "entity:uuid"
  name: "John Doe"
  entity_type_id: "entity-type:PER"
  confidence: 0.95
  mentions: 3
}

NodeType::EntityType {
  id: "entity-type:PER"
  name: "PERSON"
  description: "Named person"
  entities_count: 124
}
```

**Edge Types**:
- `HasType`: Entity -[:HAS_TYPE]-> EntityType
- `Mentions`: Chunk -[:MENTIONS]-> Entity with `weight: count` (NEW)
- `RelatedEntity`: Entity -[:RELATED_ENTITY]-> Entity (fuzzy-matched similar)

**Implementation**:
- Rename `extract_and_link_concepts()` → `extract_and_create_entities()`
- For each NER chunk:
  1. Get or create EntityType node
  2. For each entity mention in chunk:
     - Count occurrences (e.g., "John" appears 3 times)
     - Get or create Entity node
     - Create Chunk -[:MENTIONS]-> Entity edge with `weight: count`
  3. Link similar entities (fuzzy matching)
- Tests: Entity node creation, edge creation with weights, similarity linking
- Effort: 3-4 hours

**Entity Reference Weighting** (NEW):
- Chunk -[:MENTIONS]-> Entity edge weight = reference_count
- Example: "John Doe" mentioned 3 times in chunk → weight = 3.0
- Use case: Rank entities by mention frequency (important entities = higher weight)

**Feature Flag**: `ner` (enabled in `smart` features by Phase 3.5+)

**Outcome**: Entity graph fully functional. Rich entity relationships. NER feature independent from auto-tagging.

---

## Phase 4: Multiple Backend Support (FEATURE)
**Goal**: Enable multiple backends + dynamic routing

- **4.1** Extend config parser
  - Support multi-backend TOML structure
  - Validate multiple SQLite DBs, multiple Neo4j instances
  - Estimated: 2 hours

- **4.2** Add `--backend` flag to all commands (if not already from Phase -1)
  - All commands accept --backend NAME
  - Resolve to correct DB instance from config
  - Estimated: 2 hours

- **4.3** CLI context routing
  - Load correct backend from config
  - Pass to all DB operations
  - Estimated: 1 hour

**Outcome**: Multi-backend support + CLI routing functional.

---

## Phase 5: Chunking & Embedding Guarantee (CORRECTNESS)
**Goal**: Enforce chunking + embedding on insert

- **5.1** Update DB trait contract
  - add_memory() MUST chunk before storage
  - add_memory() MUST embed before storage
  - Document immutability contract for chunks
  - Estimated: 2 hours

- **5.2** Implement chunk update cascade with generic nodes
  - update_memory() invalidates existing chunk nodes
  - Re-chunk + re-embed
  - Store new chunk UUIDs as edge properties
  - Estimated: 3 hours

- **5.3** Tests: Verify chunking/embedding happens
  - Unit test: add_memory creates chunk nodes
  - Unit test: update_memory cascades to chunks
  - Estimated: 2 hours

**Outcome**: All memories properly chunked/embedded. Updates safe.

---

## Phase 6: Auto-Enrichment Pipeline (FEATURE)

**Goal**: Re-enable auto-tagging + tag refresh on memory update

**Two Separate Features**:

### 6.1 Auto-Tagging (Independent of NER)
**Purpose**: Generate topic tags from memory content on creation

- Generate tags if user provides none
- Methods (in priority order):
  1. TinyLLAMA: Use LLM to generate tags (feature: `tinyllama`)
  2. Keyword extraction: Extract high-frequency terms (always available)
  3. Classification: Classify into predefined categories (feature: `classification`)

- For each generated tag:
  1. Get or create Tag node
  2. Create Memory -[:TAGGED_WITH]-> Tag edge with `auto_generated: true` metadata

- Only run if `no user tags provided`
- Feature Flag: `tinyllama` (primary), `keywords`, `classification`
- Tests: Auto-tag generation, Tag node creation
- Estimated: 2-3 hours

### 6.2 Tag Refresh on Memory Update (NEW)
**Purpose**: Remove outdated auto-tags, regenerate new ones when content changes

- On memory update:
  1. Remove all auto-generated tags (check edge.properties.auto_generated flag)
  2. Keep user-provided tags (check edge.properties.auto_generated == false)
  3. Re-generate auto-tags from new content (if tinyllama enabled)
  4. Create new edges with `auto_generated: true` metadata

- Distinguishing auto vs user tags:
  - Tag node: add `auto_generated: bool` field
  - Edge metadata: `{ auto_generated: true | false }`
  - Query: `db.list_tags_for_memory(id)` returns both types
  - Update logic removes auto, keeps user, regenerates auto

- Estimated: 2-3 hours

### 6.3 NER Integration (Already Done in Phase 3.5)
**Purpose**: Extract named entities + maintain entity graph

- NER already enabled in Phase 3.5
- Creates Entity nodes + Entity graph
- Finds similar entities + Entity -[:RELATED_ENTITY]-> Entity
- Feature Flag: `ner`
- Estimated: 0 hours (already complete)

### 6.4 Optional NLI Integration (Future - TBD)
**Purpose**: Classify relations between entities/memories

- Integration point currently undefined
- Could suggest Memory -[rel]-> Memory relationships
- Feature Flag: `nli` (only in `full` features)
- Decision: Define use case or keep as "future"
- Estimated: TBD

**Outcome**: Full enrichment pipeline working. Auto-tagging refreshes on update. NER + auto-tagging both functional and independent.

---

## Phase 7: Configuration Flexibility (STABILITY)
**Goal**: Support multiple backend instances per type

- **7.1** Config v2 schema
  - Support [backend.default], [backend.archive], [backend.readonly]
  - Each can specify type (sqlite/neo4j) + connection details
  - Estimated: 2 hours

- **7.2** CLI backend selection
  - `voidm remember --backend default ...`
  - `voidm search --backend archive ...`
  - Estimated: 1 hour

- **7.3** Default routing
  - Unspecified --backend uses default from config
  - Estimated: 1 hour

**Outcome**: Flexible multi-backend setup. Easy to test with isolated instances.

---

## Phase 8: Search & Polish (FEATURE + POLISH)
**Goal**: Implement multi-dimensional search + remove warnings

### 8.1 Multi-Dimensional Search (NEW)
**Purpose**: Search across title, content, tags, entities, scopes with optional parameters

**CLI Interface**:
```bash
voidm search --title "neural networks"          # Search by title only
voidm search --content "training algorithms"    # Search by content only
voidm search --tags project,research            # Search by tags
voidm search --entities "John Doe"              # Search by entities
voidm search --scope project/ai                 # Search by scope
voidm search --title "AI" --content "learning" --tags research --limit 10
```

**All parameters optional** - search returns intersection-based results

**Implementation**:
- Add new search command to voidm-cli
- Implement in DB trait (Phase 4):
  - `search_by_title_embedding(embedding, limit): Vec<(Memory, f32)>`
  - `search_by_embedding(embedding, limit): Vec<(Chunk, f32)>`
  - `get_memories_by_tags(tag_names): Vec<Memory>`
  - `get_memories_by_entities(entity_names): Vec<Memory>`
  - `get_memories_by_scopes(scope_names): Vec<Memory>`

- Ranking strategy:
  - Single filter: rank by relevance within that dimension
  - Multiple filters: memories in ALL result sets ranked highest
  - Option: Configurable weights per dimension for advanced ranking

- Estimated: 3-4 hours

### 8.2 Fix neo4rs stream warnings
  - Call .next() on all DetachedRowStream
  - Estimated: 1 hour

### 8.3 Audit all unsafe code
  - Review for correctness
  - Estimated: 1 hour

### 8.4 Final compilation check
  - cargo build --all with --warnings-as-errors
  - Estimated: 30 mins

**Outcome**: Multi-dimensional search functional. Clean build. No warnings.

---

## Memory Enrichment Pipeline

Complete pipeline showing how all optional features integrate:

```
Memory Added
  ↓
[CORE] Chunking + Embedding
  ↓
[CORE] Deduplication
  ↓
[OPTIONAL] Phase 3.5: IF feature="ner"
  ├─ Extract Named Entities (PERSON, ORG, LOC, MISC)
  ├─ Create NodeType::Entity nodes
  ├─ Create NodeType::EntityType nodes
  ├─ Link Entity -[:HAS_TYPE]-> EntityType
  ├─ Link Memory -[:CONTAINS_ENTITY]-> Entity
  └─ Link Entity -[:RELATED_ENTITY]-> Entity (fuzzy similar)
  ↓
[OPTIONAL] Phase 6: IF feature="tinyllama" AND no user tags
  ├─ Generate topic tags from content
  ├─ Get or create Tag nodes
  ├─ Link Memory -[:TAGGED_WITH]-> Tag
  └─ Result: Auto-tagged with topics
  ↓
[OPTIONAL] Phase 6: IF feature="nli" (future)
  ├─ Classify relations (if integration defined)
  └─ Link Memory -[suggested_rel]-> Memory
  ↓
[OPTIONAL] Phase 4+: IF feature="query-expansion"
  └─ Expand query terms
  ↓
[OPTIONAL] Phase 5: IF feature="reranker"
  └─ Rerank results
  ↓
Complete!

**Key**: NER and auto-tagging are INDEPENDENT
- NER creates Entity graph (named entities)
- Auto-tagging creates Tag relationships (topics)
- Both work independently or together
- No dependency between them
```

---

## Sequencing & Dependencies

```
Phase -1 (Config & Local Dev) [3.5-5.5 hours] ← CRITICAL FIRST
  ↓ (enables safe development)
Phase 0 (Generic Node/Edge Format) [3-4 days]
  ├─ Title embeddings (NEW)
  └─ Edge weights default 1.0 (NEW)
  ↓
Phase 1 (Backend Abstraction - SQLx Violations) [3-4 days]
  ├─ 1.1-1.2: Extend DB trait + fix voidm-core (1 day) [CRITICAL]
  ├─ 1.3: Fix voidm-graph (0.5-1 day)
  ├─ 1.4: Fix voidm-cli (0.5-1 day)
  ├─ 1.5-1.6: Implement in backends (0.5-1 day)
  └─ 1.7: Validation & testing (0.5-1 day)
  ↓
Phase 2 (Dead Code) [1 day]
  ↓
[Phase 3 + 5 + 6 in parallel] [3.5-4.5 days]
  Phase 3: 0.5 days (Type/Scope user-provided, no auto-linking)
  Phase 5: 1.5 days
  Phase 6: 1.5 days (includes tag refresh + tag weights)
  ↓
[Phase 4 + 7 in parallel] [2 days]
  Phase 4: Includes search trait methods for multi-dimensional search
  ↓
Phase 8 (Search + Cleanup) [4-5 hours]
  ├─ 8.1: Multi-dimensional search (3-4 hours) [NEW]
  ├─ 8.2-8.4: Warnings + cleanup (2.5-3 hours)
```

**Timeline with ALL Architecture Decisions & NEW Features**:

Originally: 24.5-28.5 days (from sqlx audit + chunk ordering + gaps)

Decisions & Features Applied:
- Decision 1 (Type/Scope user-provided): -0.25 days
- Decision 2 (Tag refresh): +0 days (fits in Phase 6)
- Decision 3 (Backend embeddings): -0.5 days
- NEW: Title embeddings: +0 days (Phase 0)
- NEW: Edge weights: +0 days (Phase 0)
- NEW: Entity mention weighting: +0 days (Phase 3.5)
- NEW: Multi-dimensional search: +1 day (Phase 8)

**Revised Total**: 24.75-28.75 days (realistically **25-29 days**)

**Critical Path**: -1 → 0 → 1 → 2 = ~11-12 days
**Parallel Phases**: 3+5+6 + 4+7 = ~4-5 days
**Total**: ~24-27 days (with parallel execution)

**Why Phase -1+0+1 come first (critical path)**:
- Phase -1: Enables safe local development (no prod contamination)
- Phase 0: Generic format foundation (all other phases depend)
- Phase 1: Backend abstraction (enables Neo4j, true multi-backend)
- Phase 1: Backend abstraction (enables true multi-backend + Neo4j)

- Default backend during dev: `dev` (safe)
- Production config never touched

---

## Definition of Done

- ✓ Local `.voidm.dev.toml` auto-detected and used during dev
- ✓ Production config never touched (~/.config/voidm/config.toml safe)
- ✓ Generic node/edge format in SQLite + Neo4j
- ✓ All DB operations via trait (zero raw SQL)
- ✓ No Postgres/Concept references
- ✓ MemoryType + Scope are graph nodes with edges
- ✓ Tags are first-class citizens (NodeType::Tag)
- ✓ Entity nodes (NodeType::Entity + NodeType::EntityType) via NER
- ✓ NER extracts entities and creates Entity graph (Phase 3.5)
- ✓ Auto-tagging generates tags independently (Phase 6)
- ✓ NER and auto-tagging work independently AND together
- ✓ Multiple backends configurable
- ✓ CLI supports --backend routing
- ✓ Chunking/embedding guaranteed on insert
- ✓ Auto-tagging + NER + NLI properly integrated
- ✓ Zero compiler warnings
- ✓ All tests passing (including feature flag combinations)
- ✓ AGENT.md + TODO_NEXT.txt + PLAN.md updated

---

## Key Insights

1. **Local Config is Shield**: During 20+ day refactor, local `.voidm.dev.toml` is the safety net preventing production contamination.

2. **Generic Format is Foundation**: Without it, voidm remains tightly coupled. With it, becomes extensible and truly backend-agnostic.

3. **Smart Detection**: Config detection runs FIRST, enabling everything else. `is_dev_environment()` check prevents users outside project from using local config.

4. **Filemind Pattern Proven**: Filemind demonstrates this works for complex domains (code search). voidm can follow exact same pattern.

5. **NER ≠ Auto-Tagging**: NER creates Entity graph (named entities). Auto-tagging creates Tag relationships (topics). Completely independent features.

6. **DB Trait Exists But Unused**: 174 sqlx violations found outside backend implementations. DB trait defined perfectly but nobody uses it. Phase 1 must fix all code to route through trait.

7. **Phases -1+0 Enable Everything**: With correct foundation + safe environment + backend abstraction, all other phases become straightforward.

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Phase 1 sqlx violations not caught | Architecture stays broken | Automated CI check: forbid sqlx in non-backend crates |
| Phase 1 signature changes break code | Compilation failures everywhere | Migrate methodically: voidm-core → voidm-graph → voidm-cli |
| Trait methods incomplete | Backend implementations inconsistent | Complete voidm-db-trait extensions before implementation |
| Phase -1 design incorrect | Blocks everything for 2+ weeks | Review local config detection logic carefully |
| Local config accidentally committed | Breaks user environments | Add to .gitignore, document in README |
| Forget to use --backend flag | Contaminate prod | Local config auto-selected by default |
| Config detection too aggressive | Users outside project affected | Smart detection: only activates with Cargo.toml/.git |
| Production config accidentally modified | Data loss | Production config never auto-detected when local exists |
| Phase 0 design incorrect | Blocks Phases 1-8 | Review filemind design carefully. Start with spike test. |
| Backward compat breaking | Existing migrations fail | Test on dev Neo4j instance first |
| Performance regression | Queries slower with generic format | Benchmark early. Add indexes on type, edge_type. |


---

## CRITICAL: SQLx Violations (Phase 1 Impact)

**New Discovery**: While optional (behind feature flags), NER & NLI are BROKEN:

### Current Broken State
- **NER**: Extracts entities → creates Concepts (being removed in Phase 2) → SILENT FAILURE
- **NLI**: Fully implemented but never called anywhere → DEAD CODE  
- **Auto-Tagging**: Commented out with TODO → DISABLED

### Phase Integration

**Phase 2 (Dead Code Removal)**:
- Add task: Disable NER feature flag (or make function a no-op)
- Reason: NER creates Concepts which are being removed
- Document: "NER disabled until Phase 3 (Tag support added)"

**Phase 3 (First-Class Citizens - Tags)**:
- Add task: Refactor NER to create Tag nodes instead of Concepts
- Add task: Implement Tag nodes (NodeType::Tag)
- Add task: Re-enable NER with new Tag system
- Add task: DEFINE NLI integration point (or remove from defaults)

**Phase 6 (Auto-Enrichment)**:
- Add task: Re-enable auto_tagger_tinyllama
- Add task: Integrate NER → Tags → Auto-tags pipeline
- Add task: Implement NLI integration (relation suggestions? memory linking?)
- Add task: Test all feature flag combinations

### Feature Flag Cleanup (Phase -1 or 0)

**Change**: Temporarily remove broken features from default

```toml
# Before (BROKEN):
standard = ["embeddings", "query-expansion", "nli", "ner", "reranker", "tinyllama", "redactor"]

# After (FIXED):
standard = ["embeddings", "query-expansion", "reranker", "tinyllama", "redactor"]
# NER + NLI re-added after Phase 3/6
```

### Success Criteria

- ✓ NER creates Tag nodes (not non-existent Concepts) after Phase 3
- ✓ NLI has defined integration point or removed from default features
- ✓ Auto-tagging pipeline re-enabled after Phase 6
- ✓ All feature flag combinations tested in CI
- ✓ Optional features WORK correctly when enabled

### Key Insight

**Optional features behind feature flags MUST WORK when enabled.**  
Currently: NER breaks, NLI unused, auto-tagging disabled.  
Required: Fix during Phases 2-6, test all combinations.

