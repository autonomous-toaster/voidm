use anyhow::Result;
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use voidm_core::crud;
use voidm_db::Database;
use std::sync::Arc;
use voidm_graph;

#[derive(Subcommand)]
pub enum GraphCommands {
    /// Execute a read-only Cypher query
    Cypher(CypherArgs),
    /// Get N-hop neighbors of a memory
    Neighbors(NeighborsArgs),
    /// Find shortest path between two memories
    Path(PathArgs),
    /// Compute PageRank for all memories
    Pagerank(PagerankArgs),
    /// Show graph statistics
    Stats,
    /// Export graph to Graphviz DOT format
    Export(ExportArgs),
}

#[derive(Args)]
pub struct CypherArgs {
    /// Read-only Cypher query (MATCH/WHERE/RETURN/LIMIT). Write clauses are rejected.
    pub query: String,
}

#[derive(Args)]
pub struct NeighborsArgs {
    /// Memory ID to start from
    pub id: String,
    /// Traversal depth (default: 1)
    #[arg(long, default_value = "1")]
    pub depth: u8,
    /// Filter by relationship type: SUPPORTS, CONTRADICTS, DERIVED_FROM, PRECEDES, PART_OF, EXEMPLIFIES, INVALIDATES, RELATES_TO
    #[arg(long)]
    pub rel: Option<String>,
}

#[derive(Args)]
pub struct PathArgs {
    /// Source memory ID
    pub from: String,
    /// Target memory ID
    pub to: String,
}

#[derive(Args)]
pub struct PagerankArgs {
    /// Number of top results to return
    #[arg(long, default_value = "10")]
    pub top: usize,
    #[arg(long, default_value = "0.85")]
    pub damping: f64,
    #[arg(long, default_value = "20")]
    pub iterations: u32,
}

#[derive(Args)]
pub struct ExportArgs {
    /// Export format: dot (Graphviz), json, csv, html (interactive viewer)
    #[arg(long, default_value = "dot")]
    pub format: String,
    /// Filter: include only memories (m), concepts (c), or both (mc)
    #[arg(long)]
    pub nodes: Option<String>,
    /// Minimum edge count to include node (only nodes with >= edges shown)
    #[arg(long, default_value = "0")]
    pub min_edges: usize,
}

pub async fn run(cmd: GraphCommands, db: &std::sync::Arc<dyn voidm_db::Database>, pool: &sqlx::SqlitePool, json: bool) -> Result<()> {
    match cmd {
        GraphCommands::Cypher(args) => run_cypher(args, pool, json).await,
        GraphCommands::Neighbors(args) => run_neighbors(args, db, pool, json).await,
        GraphCommands::Path(args) => run_path(args, db, pool, json).await,
        GraphCommands::Pagerank(args) => run_pagerank(args, pool, json).await,
        GraphCommands::Stats => run_stats(db, pool, json).await,
        GraphCommands::Export(args) => run_export(args, db, pool, json).await,
    }
}

async fn run_cypher(args: CypherArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let rows = voidm_graph::cypher_read(pool, &args.query).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        for row in &rows {
            let line: Vec<String> = row.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
            println!("{}", line.join("  |  "));
        }
        println!("{} row(s)", rows.len());
    }
    Ok(())
}

async fn run_neighbors(args: NeighborsArgs, db: &Arc<dyn Database>, pool: &SqlitePool, json: bool) -> Result<()> {
    let id = match crud::resolve_id(db.as_ref(), &args.id).await {
        Ok(id) => id,
        Err(e) => {
            if json {
                println!("{}", serde_json::json!({ "error": e.to_string(), "id": args.id }));
            } else {
                eprintln!("Error: {}", e);
            }
            std::process::exit(1);
        }
    };
    let results = voidm_graph::neighbors(pool, &id, args.depth, args.rel.as_deref()).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        if results.is_empty() {
            println!("No neighbors found for '{}' at depth {}.", id, args.depth);
            println!("Hint: Use 'voidm link {} <EDGE_TYPE> <target-id>' to create edges.", &id);
        } else {
            for n in &results {
                println!("[depth {}] {} --[{}]--> {} ({})", n.depth, &id, n.rel_type, n.memory_id, n.direction);
            }
            println!("{} neighbor(s)", results.len());
        }
    }
    Ok(())
}

async fn run_path(args: PathArgs, db: &Arc<dyn Database>, pool: &SqlitePool, json: bool) -> Result<()> {
    // Resolve both IDs before same-ID check (so short IDs expand correctly)
    let from = crud::resolve_id(db.as_ref(), &args.from).await?;
    let to   = crud::resolve_id(db.as_ref(), &args.to).await?;

    if from == to {
        if json {
            println!("{}", serde_json::json!({
                "error": "Source and target IDs are the same. A path requires two distinct memories.",
                "from": from, "to": to
            }));
        } else {
            eprintln!("Error: Source and target IDs are the same ('{}').\nA path requires two distinct memory IDs.", &from);
        }
        std::process::exit(2);
    }

    match voidm_graph::shortest_path(pool, &from, &to).await? {
        None => {
            if json {
                println!("{}", serde_json::json!({
                    "path": null,
                    "message": format!("No path found between '{}' and '{}'", &from, &to),
                    "hint": "Memories may not be connected. Use 'voidm link' to create edges."
                }));
            } else {
                println!("No path found between '{}' and '{}'.", &from, &to);
                println!("Hint: Use 'voidm link {} <EDGE_TYPE> {}' to connect them.", &from, &to);
            }
        }
        Some(path) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&path)?);
            } else {
                let parts: Vec<String> = path.iter().map(|s| {
                    if let Some(ref r) = s.rel_type {
                        format!("{} -[{}]->", s.memory_id, r)
                    } else {
                        s.memory_id.clone()
                    }
                }).collect();
                println!("{}", parts.join(" "));
            }
        }
    }
    Ok(())
}

async fn run_pagerank(args: PagerankArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let mut results = voidm_graph::pagerank(pool, args.damping, args.iterations).await?;
    results.truncate(args.top);
    if json {
        let v: Vec<_> = results.iter()
            .map(|(id, score)| serde_json::json!({"id": id, "score": score}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        if results.is_empty() {
            println!("No memories in graph yet. Use 'voidm add' and 'voidm link' to build the graph.");
        } else {
            for (i, (id, score)) in results.iter().enumerate() {
                println!("#{} [{:.4}] {}", i + 1, score, id);
            }
        }
    }
    Ok(())
}

async fn run_stats(db: &Arc<dyn Database>, _pool: &SqlitePool, json: bool) -> Result<()> {
    let stats = db.get_graph_stats().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "node_count": stats.node_count,
            "edge_count": stats.edge_count,
            "rel_type_counts": stats.edges_by_type.iter().map(|(t, c)| (t.clone(), c)).collect::<std::collections::HashMap<_, _>>()
        }))?);
    } else {
        println!("Nodes: {}", stats.node_count);
        println!("Edges: {}", stats.edge_count);
        if !stats.edges_by_type.is_empty() {
            println!("Edge types:");
            let mut counts: Vec<_> = stats.edges_by_type.iter().collect();
            counts.sort_by(|a, b| b.1.cmp(&a.1));
            for (rel, cnt) in counts {
                println!("  {:20} {}", rel, cnt);
            }
        } else {
            println!("No edges yet. Use 'voidm link <id> <EDGE_TYPE> <id>' to create edges.");
        }
    }
    Ok(())
}

async fn run_export(args: ExportArgs, db: &Arc<dyn Database>, _pool: &SqlitePool, _json: bool) -> Result<()> {
    match args.format.as_str() {
        "dot" => export_dot(db, args).await,
        "json" => export_json(db, args).await,
        "csv" => export_csv(db, args).await,
        fmt => anyhow::bail!("Unknown format: {}. Supported: dot, json, csv", fmt),
    }
}

async fn export_dot(db: &Arc<dyn Database>, _args: ExportArgs) -> Result<()> {
    let data = db.get_graph_export_data().await?;

    // Start DOT file
    println!("digraph voidm {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box, style=rounded];");
    
    // Add memory nodes
    for mem in &data.memories {
        let color = match mem.mem_type.as_str() {
            "semantic" => "lightblue",
            "episodic" => "lightgreen",
            "procedural" => "lightyellow",
            "conceptual" => "lightcyan",
            "contextual" => "lightgray",
            _ => "white",
        };
        let label = mem.preview.replace("\"", "\\\"");
        let id_short = if mem.id.len() >= 8 { &mem.id[..8] } else { &mem.id };
        println!("  \"m:{}\" [label=\"{}\", fillcolor=\"{}\", style=\"rounded,filled\"];", 
                 id_short, label, color);
    }

    // Add concept nodes
    for concept in &data.concepts {
        let label = concept.name.replace("\"", "\\\"");
        let id_short = if concept.id.len() >= 8 { &concept.id[..8] } else { &concept.id };
        println!("  \"c:{}\" [label=\"{} (concept)\", fillcolor=\"lavender\", style=\"rounded,filled\"];", 
                 id_short, label);
    }

    // Add edges
    for edge in &data.edges {
        let from_node = if edge.from_id.starts_with("m:") { 
            edge.from_id.clone() 
        } else { 
            let id_short = if edge.from_id.len() >= 8 { &edge.from_id[..8] } else { &edge.from_id };
            format!("m:{}", id_short) 
        };
        let to_node = if edge.to_id.starts_with("c:") { 
            edge.to_id.clone() 
        } else { 
            let id_short = if edge.to_id.len() >= 8 { &edge.to_id[..8] } else { &edge.to_id };
            format!("c:{}", id_short) 
        };
        println!("  \"{}\" -> \"{}\" [label=\"{}\"];", from_node, to_node, edge.rel_type);
    }

    println!("}}");
    Ok(())
}

async fn export_json(db: &Arc<dyn Database>, _args: ExportArgs) -> Result<()> {
    use serde_json::json;
    
    let data = db.get_graph_export_data().await?;

    let result = json!({
        "memories": data.memories.iter().map(|m| json!({"id": m.id, "type": m.mem_type})).collect::<Vec<_>>(),
        "concepts": data.concepts.iter().map(|c| json!({"id": c.id, "name": c.name})).collect::<Vec<_>>(),
        "edges": data.edges.iter().map(|e| json!({"from": e.from_id, "to": e.to_id, "type": e.rel_type})).collect::<Vec<_>>(),
    });

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn export_csv(db: &Arc<dyn Database>, _args: ExportArgs) -> Result<()> {
    let data = db.get_graph_export_data().await?;

    println!("from_id,to_id,relationship_type");
    for edge in data.edges {
        println!("{},{},{}", edge.from_id, edge.to_id, edge.rel_type);
    }
    Ok(())
}
