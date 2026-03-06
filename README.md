# voidm

Local-first persistent memory for LLM agents.

`voidm` is a single-binary CLI that gives AI agents a durable memory store: add typed memories, search them with hybrid vector+BM25+fuzzy retrieval, connect them in a knowledge graph, and query with Cypher — all offline, no API keys required.

---

## Features

- **Hybrid search** — vector (ANN), BM25, fuzzy, keyword, or combined with RRF scoring
- **Knowledge graph** — link memories with typed directed edges (SUPPORTS, DERIVED_FROM, PART_OF, …)
- **Cypher queries** — read-only graph traversal with a minimal recursive-descent parser
- **Local embeddings** — [fastembed](https://github.com/Anush008/fastembed-rs) + ONNX, `all-MiniLM-L6-v2` by default, no network after first download
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

### Link memories together

Memories can be connected with typed edges to build a knowledge graph:

```bash
# The procedural runbook was derived from the semantic fact
voidm link <runbook-id> DERIVED_FROM <migration-fact-id>

# The conceptual decision supports the semantic observation
voidm link <decision-id> SUPPORTS <fact-id>

# Generic association — note is required
voidm link <id1> RELATES_TO <id2> --note "both affect deploy order"
```

When you add a memory, `voidm` returns `suggested_links` (similarity ≥ 0.7) and flags `duplicate_warning` (similarity ≥ 0.95) — so linking can happen naturally as part of the add workflow.

### Explore the graph

```bash
# Neighbors of a memory, 2 hops out
voidm graph neighbors <id> --depth 2

# Which memories are most connected?
voidm graph pagerank --top 10

# Cypher: find all memories that support another
voidm graph cypher "MATCH (a:Memory)-[:SUPPORTS]->(b:Memory) RETURN a.memory_id AS from, b.memory_id AS to LIMIT 20"

# Cypher: edges with their types
voidm graph cypher "MATCH (a:Memory)-[r]->(b:Memory) RETURN a.memory_id AS from, r.rel_type AS rel, b.memory_id AS to LIMIT 20"

# Cypher: filter by a specific node
voidm graph cypher "MATCH (a:Memory)-[r]->(b:Memory) WHERE a.memory_id = '<id>' RETURN r.rel_type AS rel, b.memory_id AS to"
```

Supported Cypher clauses: `MATCH`, `WHERE`, `RETURN`, `ORDER BY`, `LIMIT`, `WITH`. Write operations are rejected.

### For agents: `voidm instructions`

```bash
voidm instructions        # full usage guide in markdown
voidm instructions --json # machine-readable: types, edge hints, workflow
```

Prints a structured guide covering memory types, edge selection, the insertion workflow, and Cypher examples. Useful for bootstrapping an agent's system prompt or tool description.

---

## CLI Reference

| Command | Description |
|---------|-------------|
| `voidm add` | Add a memory. Returns `suggested_links` and `duplicate_warning`. |
| `voidm get <id>` | Retrieve a memory by ID or short prefix. |
| `voidm delete <id>` | Delete a memory. |
| `voidm list` | List memories, optionally filtered by scope or type. |
| `voidm search <query>` | Hybrid search. Modes: `hybrid`, `semantic`, `bm25`, `fuzzy`, `keyword`. |
| `voidm link <from> <EDGE> <to>` | Create a graph edge. `RELATES_TO` requires `--note`. |
| `voidm unlink <from> <EDGE> <to>` | Remove a graph edge. |
| `voidm graph neighbors <id>` | N-hop neighbors. |
| `voidm graph pagerank` | Rank memories by graph centrality. |
| `voidm graph cypher "<query>"` | Read-only Cypher query. |
| `voidm graph path <from> <to>` | Shortest path between two memories. |
| `voidm graph stats` | Edge counts by type. |
| `voidm export` | Export memories as JSON. |
| `voidm models list` | List available embedding models. |
| `voidm models reembed` | Re-embed all memories with current model. |
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
│   ├── voidm-core/    # DB, embeddings, CRUD, hybrid search, config
│   ├── voidm-graph/   # EAV graph schema, Cypher parser + translator
│   └── voidm-cli/     # Clap CLI, JSON/table output
└── migrations/        # SQLite schema (sqlx)
```

- **Storage:** `~/.local/share/voidm/memories.db` (single SQLite file)
- **Config:** `~/.config/voidm/config.toml`
- **Search pipeline:** Vector ANN (sqlite-vec) + BM25 (FTS5) + fuzzy (strsim) → RRF merge
- **Graph:** Pure SQLx EAV schema — no external graph DB, fully transactional

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
