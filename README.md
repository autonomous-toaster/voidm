# voidm

**Local-first persistent memory for LLM agents.**

`voidm` is a single-binary CLI tool that gives AI agents a semantic memory store: add memories, search them with hybrid vector+BM25+fuzzy retrieval, link them in a knowledge graph, and query the graph with Cypher — all offline, no API keys required.

```
voidm add "Always run migrations before deploying" --type procedural
voidm search "deployment checklist"
voidm graph cypher "MATCH (a:Memory)-[r]->(b:Memory) RETURN a.memory_id AS from, r.rel_type AS rel, b.memory_id AS to LIMIT 10"
```

---

## Features

- **Hybrid search** — vector (ANN), BM25 (full-text), fuzzy, keyword, or combined
- **Knowledge graph** — link memories with typed edges (SUPPORTS, DERIVED_FROM, PART_OF, …)
- **Cypher queries** — read-only graph traversal with a hand-rolled parser
- **Local embeddings** — `fastembed` + ONNX, default model `all-MiniLM-L6-v2` (384 dims), no API key
- **Auto-init** — DB created on first write, no `init` step needed
- **Short IDs** — use any 4+ char prefix instead of full UUIDs
- **JSON output** — every command supports `--json` for machine consumption

---

## Installation

### From source

```bash
cargo install --path crates/voidm-cli
```

Or build manually:

```bash
cargo build --release
cp target/release/voidm ~/.local/bin/
```

> **Requirements:** Rust 1.70+, no system dependencies (SQLite is bundled).

---

## Quick Start

```bash
# Add memories
voidm add "Postgres chosen for ACID guarantees" --type conceptual --scope work/acme
voidm add "DB migration takes ~5 min" --type semantic --scope work/acme
voidm add "Run: rake db:migrate then restart puma" --type procedural --scope work/acme

# Search
voidm search "database migration"
voidm search "database" --scope work/acme --mode semantic --json

# Link memories
voidm link <id1> SUPPORTS <id2>
voidm link <id1> RELATES_TO <id2> --note "both affect deploy order"

# Graph
voidm graph neighbors <id> --depth 2
voidm graph pagerank --top 10
voidm graph cypher "MATCH (a:Memory)-[:SUPPORTS]->(b:Memory) RETURN a.memory_id, b.memory_id LIMIT 20"

# Inspect
voidm info
voidm stats
```

---

## CLI Reference

### `voidm add`

```
voidm add <content> --type <type> [--scope <scope>] [--tags <tag1,tag2>] [--importance 1-10]
          [--link <id>:<EDGE>:<note>] [--json]
```

Returns the new memory ID plus `suggested_links` (similarity ≥ 0.7) and `duplicate_warning` (similarity ≥ 0.95).

**Memory types:** `episodic` | `semantic` | `procedural` | `conceptual` | `contextual`

### `voidm search`

```
voidm search <query> [--mode hybrid|semantic|bm25|fuzzy|keyword]
             [--scope <scope>] [--min-score 0-1] [--limit N] [--json]
```

Default mode `hybrid` filters at `min-score 0.3`. Other modes return unfiltered scores.

### `voidm list`

```
voidm list [--scope <scope>] [--type <type>] [--limit N] [--json]
```

### `voidm get`

```
voidm get <id> [--json]
```

### `voidm delete`

```
voidm delete <id> [--yes] [--json]
```

### `voidm link` / `voidm unlink`

```
voidm link <from-id> <EDGE_TYPE> <to-id> [--note <reason>]
voidm unlink <from-id> <EDGE_TYPE> <to-id>
```

**Edge types:** `SUPPORTS` | `CONTRADICTS` | `DERIVED_FROM` | `PRECEDES` | `PART_OF` | `EXEMPLIFIES` | `INVALIDATES` | `RELATES_TO` (requires `--note`)

### `voidm graph`

```
voidm graph neighbors <id> [--depth N] [--json]
voidm graph pagerank [--top N] [--json]
voidm graph cypher "<query>" [--json]
voidm graph stats [--json]
voidm graph path <from-id> <to-id> [--json]
```

**Cypher** supports: `MATCH`, `WHERE`, `RETURN`, `ORDER BY`, `LIMIT`, `WITH`.  
Write clauses (`CREATE`, `MERGE`, `SET`, `DELETE`, …) are rejected (exit 2).  
Node properties: `memory_id`, `type`, `importance`, `created_at`.  
Edge properties: `rel_type`, `note`.

### `voidm export`

```
voidm export [--scope <scope>] [--output <file>]
```

Exports memories as JSON.

### `voidm models`

```
voidm models list
voidm models reembed [--model <model-name>]
```

### `voidm config`

```
voidm config show
voidm config set <key> <value>
```

### `voidm info` / `voidm stats`

```
voidm info   # DB path, config path, embedding model, search defaults
voidm stats  # Memory counts by type, embedding coverage, top tags, graph edges, DB size
```

### `voidm instructions`

```
voidm instructions [--json]
```

Prints the full agent usage guide (markdown or JSON). Useful for bootstrapping an agent's system prompt.

---

## Architecture

```
voidm/
├── crates/
│   ├── voidm-core/    # DB, embeddings, CRUD, hybrid search, config
│   ├── voidm-graph/   # EAV graph schema, Cypher parser/translator
│   └── voidm-cli/     # Clap CLI, JSON/table output
└── migrations/        # SQLite schema migrations (sqlx)
```

**Storage:** Single SQLite file at `~/.local/share/voidm/memories.db`  
**Config:** `~/.config/voidm/config.toml`  
**Embeddings:** [fastembed](https://github.com/Anush008/fastembed-rs) + ONNX Runtime (local, no network after first model download)  
**Graph:** Pure SQLx EAV schema — no external graph DB, fully transactional  
**Search pipeline:** Vector ANN (sqlite-vec) → BM25 (FTS5) → fuzzy (strsim) → RRF merge  

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Not found |
| 2 | Error (bad args, write Cypher rejected, missing required field) |

---

## License

MIT — see [LICENSE](LICENSE).
