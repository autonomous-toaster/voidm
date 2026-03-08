# voidm

Local-first persistent memory for LLM agents.

`voidm` is a single-binary CLI that gives AI agents a durable memory store: add typed memories, search them with hybrid vector+BM25+fuzzy retrieval, connect them in a knowledge graph, define ontology concepts with IS-A hierarchies, extract entities with a local NER model, detect contradictions with a local NLI model, and query with Cypher — all offline, no API keys required.

---

## Features

- **Hybrid search** — vector (ANN), BM25, fuzzy, keyword, or combined with RRF scoring
- **Quality scoring** — automatic scoring (0.0-1.0) for all memories; filtering by quality threshold
- **Knowledge graph** — link memories with typed directed edges (SUPPORTS, DERIVED_FROM, PART_OF, …)
- **Ontology layer** — first-class concept nodes, IS-A hierarchies, INSTANCE_OF links, subsumption queries
- **Concept deduplication** — manual merge, auto-detection, prevention at creation time; batch merge operations
- **Graph visualization** — export as interactive HTML, DOT (Graphviz), JSON, CSV; force-directed layout
- **Local NER** — entity extraction via `Xenova/bert-base-NER` (ONNX, ~103 MB, downloaded once)
- **Local NLI** — relation classification + contradiction detection via `cross-encoder/nli-deberta-v3-small`
- **Conflict management** — surface CONTRADICTS edges, resolve with INVALIDATES
- **Cypher queries** — read-only graph traversal; `:Memory` and `:Concept` node labels both supported
- **Local embeddings** — [fastembed](https://github.com/Anush008/fastembed-rs) + ONNX, 7 models available
- **Model initialization** — `voidm init` pre-downloads all models for offline use (CI-friendly)
- **Auto-init** — DB created on first write, no setup step
- **Short IDs** — use any 4+ char UUID prefix instead of full IDs
- **JSON output** — every command supports `--json` for agent consumption

---

## Installation

```bash
git clone https://github.com/autonomous-toaster/voidm
cd voidm
cargo build --release
cp target/release/voidm ~/.local/bin/
```

> Requires Rust 1.70+. SQLite is bundled — no system dependencies.  
> ML models are downloaded on first use to `~/.cache/voidm/`.

### Model Initialization (Optional)

To pre-download models for offline use (useful for CI):

```bash
voidm init
```

This downloads the configured embedding model (default: `Xenova/all-MiniLM-L6-v2`), NER, and NLI models to `~/.cache/voidm/models/`. Total: ~300-400 MB. Idempotent—skips already-cached models.

If you change the embedding model later via `voidm config set embeddings.model <name>`, the new model will be automatically downloaded on first use.

---

## Usage

### Add memories

```bash
voidm add "Postgres chosen for ACID guarantees" --type conceptual --scope work/acme
voidm add "DB migration takes ~5 min on production" --type semantic --scope work/acme
voidm add "Run rake db:migrate then restart puma" --type procedural --scope work/acme
```

### Search

```bash
voidm search "deployment"
voidm search "database" --scope work/acme --mode semantic
voidm search "migration" --min-score 0 --limit 20 --json
```

Filter by quality score (0.0-1.0, added automatically):

```bash
# Only high-quality memories (0.8+)
voidm search "pattern" --min-quality 0.8 --limit 10

# All memories regardless of quality
voidm search "pattern" --min-quality 0.0
```

Quality scores reflect genericity, abstraction, temporal independence, and substance. Use `--min-quality` to skip low-confidence memories.

### Link memories together

```bash
voidm link <runbook-id> DERIVED_FROM <migration-fact-id>
voidm link <decision-id> SUPPORTS <fact-id>
voidm link <id1> RELATES_TO <id2> --note "both affect deploy order"
```

When you add a memory, `voidm` returns `suggested_links` (similarity ≥ 0.7) and flags `duplicate_warning` (similarity ≥ 0.95).

### Explore the graph

```bash
voidm graph neighbors <id> --depth 2
voidm graph pagerank --top 10
voidm graph cypher "MATCH (a:Memory)-[:SUPPORTS]->(b:Memory) RETURN a.memory_id, b.memory_id LIMIT 20"
voidm graph cypher "MATCH (c:Concept) WHERE c.name = 'AuthService' RETURN c.id, c.description"
```

Supported Cypher clauses: `MATCH`, `WHERE`, `RETURN`, `ORDER BY`, `LIMIT`, `WITH`. Write operations are rejected. Both `:Memory` and `:Concept` node labels are supported.

### Export and visualize the graph

```bash
# Export as interactive HTML (force-directed, searchable, filterable)
voidm graph export --format html > graph.html
open graph.html

# Export as DOT (Graphviz format)
voidm graph export --format dot > graph.dot
dot -Tsvg graph.dot -o graph.svg

# Export as JSON (for custom tools)
voidm graph export --format json > graph.json

# Export as CSV (edge list, for spreadsheets)
voidm graph export --format csv > edges.csv
```

---

## Ontology

The ontology layer adds first-class concept nodes — classes, categories, architectural components — that memories can be attached to as instances.

### Define concepts

```bash
# Create a concept class
voidm ontology concept add "AuthService" --description "Handles JWT + OAuth2 flows" --scope work/acme

# List concepts
voidm ontology concept list --scope work/acme

# Get a concept with its instances, subclasses, and superclasses
voidm ontology concept get <id>
```

### IS-A hierarchies

Concepts can form class hierarchies via IS_A edges. Subsumption is computed with recursive CTEs — querying a parent returns all instances of all subclasses too.

```bash
voidm ontology link <child-concept-id> --from-kind concept \
  IS_A <parent-concept-id> --to-kind concept
```

### Link memories to concepts

```bash
# Make a memory an instance of a concept class
voidm ontology link <memory-id> --from-kind memory \
  INSTANCE_OF <concept-id> --to-kind concept

# Query all instances (transitive — includes subclass instances)
voidm ontology concept get <concept-id>
```

Ontology edge types: `IS_A`, `INSTANCE_OF`, `HAS_PROPERTY`, `CONTRADICTS`, `INVALIDATES`.

### Batch NER enrichment

Extract named entities from all stored memories and auto-link them to matching concepts:

```bash
voidm ontology enrich-memories              # process all unprocessed memories
voidm ontology enrich-memories --scope work/acme --add   # also create missing concepts
voidm ontology enrich-memories --force      # reprocess already-processed memories
voidm ontology enrich-memories --dry-run    # preview without writing
voidm ontology enrich-memories --limit 50   # cap at N memories
```

The NER model (`Xenova/bert-base-NER`) is downloaded once to `~/.cache/voidm/ner/`. A tracking table (`ontology_ner_processed`) prevents redundant re-runs.

### Extract entities from a single memory

```bash
voidm ontology extract <memory-id>
voidm ontology extract <memory-id> --add --min-score 0.8
```

### NLI-based enrichment

Use a local NLI model to classify relations between two texts and detect contradictions:

```bash
voidm ontology enrich <text1> <text2>
voidm ontology concept add "..." --enrich   # enrich at creation time
```

The NLI model (`cross-encoder/nli-deberta-v3-small`) is downloaded once to `~/.cache/voidm/nli/`. Contradiction threshold: 0.80.

### Concept Deduplication

voidm detects and merges duplicate concepts in three ways:

#### 1. Manual Merge
```bash
voidm ontology concept merge <source-id> <target-id>
# Retargets all INSTANCE_OF and IS_A edges from source to target, then deletes source
```

#### 2. Auto-Detection
```bash
voidm ontology concept find-merge-candidates --threshold 0.90
# Lists concept pairs with > 90% name similarity

voidm ontology concept find-merge-candidates --threshold 0.90 --output candidates.json
# Save to file for batch processing
```

#### 3. Batch Merge (Preview & Execute)
```bash
# Preview impact without changing anything
voidm ontology concept merge-batch --from candidates.json

# Execute the merges
voidm ontology concept merge-batch --from candidates.json --execute

# View merge history
voidm ontology concept merge-history

# Rollback a merge if needed
voidm ontology concept rollback-merge <merge-id>
```

#### 4. Prevention at Creation Time
When adding a concept, similar existing concepts are checked and reported:
```bash
voidm ontology concept add "DatabaseConnection"
# Warning: Similar concepts found (consider merging):
#   - Database (87% similar, 5 edges)
#   - DBConnection (94% similar, 3 edges)
```

---

## Conflict Management

Contradicting concepts surface as `CONTRADICTS` edges. Review and resolve them with:

```bash
# List all unresolved conflicts
voidm conflicts list
voidm conflicts list --scope work/acme

# Resolve: keep the winner, mark the loser as [SUPERSEDED]
voidm conflicts resolve <edge-id> --keep <winning-concept-id>
```

Resolving replaces the `CONTRADICTS` edge with an `INVALIDATES` edge (winner → loser) and prepends `[SUPERSEDED]` to the loser's description.

---

## CLI Reference

### Memory

| Command | Description |
|---------|-------------|
| `voidm add` | Add a memory. Returns `suggested_links` and `duplicate_warning`. |
| `voidm get <id>` | Retrieve a memory by ID or short prefix. |
| `voidm delete <id>` | Delete a memory. |
| `voidm list` | List memories, filtered by scope or type. |
| `voidm search <query>` | Hybrid search. Modes: `hybrid`, `semantic`, `bm25`, `fuzzy`, `keyword`. |
| `voidm link <from> <EDGE> <to>` | Create a graph edge. `RELATES_TO` requires `--note`. |
| `voidm unlink <from> <EDGE> <to>` | Remove a graph edge. |
| `voidm export` | Export memories as JSON. |

### Graph

| Command | Description |
|---------|-------------|
| `voidm graph neighbors <id>` | N-hop neighbors (`--depth`, default 1). |
| `voidm graph pagerank` | Rank memories + concepts by graph centrality. |
| `voidm graph cypher "<query>"` | Read-only Cypher. `:Memory` and `:Concept` labels supported. |
| `voidm graph path <from> <to>` | Shortest path between two memories. |
| `voidm graph stats` | Edge counts by type. |
| `voidm graph export --format <fmt>` | Export graph. Formats: `html` (interactive), `dot` (Graphviz), `json`, `csv`. |

### Ontology

| Command | Description |
|---------|-------------|
| `voidm ontology concept add <name>` | Create a concept. `--description`, `--scope`. |
| `voidm ontology concept get <id>` | Get concept with instances, subclasses, superclasses. |
| `voidm ontology concept list` | List concepts. `--scope`. |
| `voidm ontology concept delete <id>` | Delete a concept. |
| `voidm ontology link <from> <EDGE> <to>` | Create ontology edge. `--from-kind`, `--to-kind` (memory\|concept). |
| `voidm ontology unlink <from> <EDGE> <to>` | Remove ontology edge. |
| `voidm ontology edges <id>` | List edges for a concept. |
| `voidm ontology hierarchy <id>` | Full IS-A hierarchy for a concept. |
| `voidm ontology instances <id>` | All instances (transitive). |
| `voidm ontology extract <id>` | Extract NER entities from a memory. `--add`, `--min-score`. |
| `voidm ontology enrich-memories` | Batch NER enrichment. `--scope`, `--add`, `--force`, `--dry-run`, `--limit`. |
| `voidm ontology enrich <text1> <text2>` | NLI relation classification between two texts. |
| `voidm ontology concept merge <src> <tgt>` | Manually merge source concept into target. |
| `voidm ontology concept find-merge-candidates` | Auto-detect duplicates. `--threshold` (0.0-1.0), `--output` (JSON file). |
| `voidm ontology concept merge-batch --from <file>` | Preview or execute batch merge. Add `--execute` to apply. |
| `voidm ontology concept merge-history` | View merge audit trail. Filter: `--batch`, `--status`. |
| `voidm ontology concept rollback-merge <id>` | Undo a merge operation. |
| `voidm ontology benchmark` | NLI model benchmark on built-in test pairs. |

### Conflicts

| Command | Description |
|---------|-------------|
| `voidm conflicts list` | List unresolved CONTRADICTS edges. `--scope`. |
| `voidm conflicts resolve <edge-id>` | Resolve conflict. `--keep <winning-id>`. |

### System

| Command | Description |
|---------|-------------|
| `voidm models list` | List available embedding models. |
| `voidm models reembed` | Re-embed all memories with current model. |
| `voidm init` | Pre-download all models to `~/.cache/voidm/models/`. Idempotent. |
| `voidm config show/set` | Show or update config. |
| `voidm info` | DB path, config path, model, search defaults. |
| `voidm stats` | Memory counts, embedding coverage, top tags, DB size. |
| `voidm instructions` | Print agent usage guide. |

Use `--json` on any command for machine-readable output. Use `--help` for full flag reference.

---

## Architecture

```
voidm/
├── crates/
│   ├── voidm-core/    # DB, embeddings, CRUD, hybrid search, ontology, NER, NLI, config
│   ├── voidm-graph/   # EAV graph schema, Cypher parser + translator (:Memory + :Concept)
│   └── voidm-cli/     # Clap CLI, JSON/table output, all subcommands
└── migrations/        # SQLite schema (sqlx)
```

- **Storage:** `~/.local/share/voidm/memories.db` (single SQLite file)
- **Config:** `~/.config/voidm/config.toml`
- **ML cache:** `~/.cache/voidm/` (NER + NLI ONNX models, downloaded on first use)
- **Search pipeline:** Vector ANN (sqlite-vec) + BM25 (FTS5) + fuzzy (strsim) → RRF merge
- **Graph:** Pure SQLx EAV schema — no external graph DB, fully transactional
- **Ontology:** `ontology_concepts` + `ontology_edges` tables; recursive CTE subsumption
- **NER:** `Xenova/bert-base-NER` quantized ONNX (~103 MB); subword span stitching for CamelCase
- **NLI:** `cross-encoder/nli-deberta-v3-small` ONNX; contradiction threshold 0.80

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Not found |
| `2` | Error (bad args, write Cypher rejected, missing required field) |

---

## Acknowledgements

Inspired by [byteowlz/mmry](https://github.com/byteowlz/mmry) and [colliery-io/graphqlite](https://github.com/colliery-io/graphqlite).

Built with ❤️ and [pi-coding-agent](https://github.com/badlogic/pi-mono).

---

## License

MIT — see [LICENSE](LICENSE).
