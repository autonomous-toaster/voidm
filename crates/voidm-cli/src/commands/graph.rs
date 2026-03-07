use anyhow::Result;
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use voidm_core::resolve_id;
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
    /// Open interactive HTML viewer in default browser
    Show,
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

pub async fn run(cmd: GraphCommands, pool: &SqlitePool, json: bool) -> Result<()> {
    match cmd {
        GraphCommands::Cypher(args) => run_cypher(args, pool, json).await,
        GraphCommands::Neighbors(args) => run_neighbors(args, pool, json).await,
        GraphCommands::Path(args) => run_path(args, pool, json).await,
        GraphCommands::Pagerank(args) => run_pagerank(args, pool, json).await,
        GraphCommands::Stats => run_stats(pool, json).await,
        GraphCommands::Export(args) => run_export(args, pool, json).await,
        GraphCommands::Show => run_show(pool).await,
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

async fn run_neighbors(args: NeighborsArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    let id = match resolve_id(pool, &args.id).await {
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
            println!("Hint: Use 'voidm link {} <EDGE_TYPE> <target-id>' to create edges.", id);
        } else {
            for n in &results {
                println!("[depth {}] {} --[{}]--> {} ({})", n.depth, id, n.rel_type, n.memory_id, n.direction);
            }
            println!("{} neighbor(s)", results.len());
        }
    }
    Ok(())
}

async fn run_path(args: PathArgs, pool: &SqlitePool, json: bool) -> Result<()> {
    // Resolve both IDs before same-ID check (so short IDs expand correctly)
    let from = resolve_id(pool, &args.from).await?;
    let to   = resolve_id(pool, &args.to).await?;

    if from == to {
        if json {
            println!("{}", serde_json::json!({
                "error": "Source and target IDs are the same. A path requires two distinct memories.",
                "from": from, "to": to
            }));
        } else {
            eprintln!("Error: Source and target IDs are the same ('{}').\nA path requires two distinct memory IDs.", from);
        }
        std::process::exit(2);
    }

    match voidm_graph::shortest_path(pool, &from, &to).await? {
        None => {
            if json {
                println!("{}", serde_json::json!({
                    "path": null,
                    "message": format!("No path found between '{}' and '{}'", from, to),
                    "hint": "Memories may not be connected. Use 'voidm link' to create edges."
                }));
            } else {
                println!("No path found between '{}' and '{}'.", from, to);
                println!("Hint: Use 'voidm link {} <EDGE_TYPE> {}' to connect them.", from, to);
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

async fn run_stats(pool: &SqlitePool, json: bool) -> Result<()> {
    let stats = voidm_graph::graph_stats(pool).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("Nodes: {}", stats.node_count);
        println!("Edges: {}", stats.edge_count);
        if !stats.rel_type_counts.is_empty() {
            println!("Edge types:");
            let mut counts: Vec<_> = stats.rel_type_counts.iter().collect();
            counts.sort_by(|a, b| b.1.cmp(a.1));
            for (rel, cnt) in counts {
                println!("  {:20} {}", rel, cnt);
            }
        } else {
            println!("No edges yet. Use 'voidm link <id> <EDGE_TYPE> <id>' to create edges.");
        }
    }
    Ok(())
}

async fn run_export(args: ExportArgs, pool: &SqlitePool, _json: bool) -> Result<()> {
    match args.format.as_str() {
        "dot" => export_dot(args, pool).await,
        "json" => export_json(args, pool).await,
        "csv" => export_csv(args, pool).await,
        "html" => export_html(args, pool).await,
        fmt => anyhow::bail!("Unknown format: {}. Supported: dot, json, csv, html", fmt),
    }
}

async fn export_dot(_args: ExportArgs, pool: &SqlitePool) -> Result<()> {
    // Get all memories
    let memories: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, type, SUBSTR(content, 1, 50) as preview FROM memories LIMIT 1000"
    )
    .fetch_all(pool)
    .await?;

    // Get all concepts
    let concepts: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, name FROM ontology_concepts LIMIT 500"
    )
    .fetch_all(pool)
    .await?;

    // Get all edges
    let edges: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT from_id, to_id, rel_type FROM ontology_edges LIMIT 2000"
    )
    .fetch_all(pool)
    .await?;

    // Start DOT file
    println!("digraph voidm {{");
    println!("  rankdir=LR;");
    println!("  node [shape=box, style=rounded];");
    
    // Add memory nodes
    for (id, mem_type, preview) in &memories {
        let color = match mem_type.as_str() {
            "semantic" => "lightblue",
            "episodic" => "lightgreen",
            "procedural" => "lightyellow",
            "conceptual" => "lightcyan",
            "contextual" => "lightgray",
            _ => "white",
        };
        let label = preview.replace("\"", "\\\"");
        println!("  \"m:{}\" [label=\"{}\", fillcolor=\"{}\", style=\"rounded,filled\"];", 
                 &id[..8], label, color);
    }

    // Add concept nodes
    for (id, name) in &concepts {
        let label = name.replace("\"", "\\\"");
        println!("  \"c:{}\" [label=\"{} (concept)\", fillcolor=\"lavender\", style=\"rounded,filled\"];", 
                 &id[..8], label);
    }

    // Add edges
    for (from, to, rel) in &edges {
        let from_node = if from.starts_with("m:") { 
            from.clone() 
        } else { 
            format!("m:{}", &from[..8]) 
        };
        let to_node = if to.starts_with("c:") { 
            to.clone() 
        } else { 
            format!("c:{}", &to[..8]) 
        };
        println!("  \"{}\" -> \"{}\" [label=\"{}\"];", from_node, to_node, rel);
    }

    println!("}}");
    Ok(())
}

async fn export_json(_args: ExportArgs, pool: &SqlitePool) -> Result<()> {
    use serde_json::json;
    
    let memories: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, type FROM memories LIMIT 1000"
    )
    .fetch_all(pool)
    .await?;

    let concepts: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, name FROM ontology_concepts LIMIT 500"
    )
    .fetch_all(pool)
    .await?;

    let edges: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT from_id, to_id, rel_type FROM ontology_edges LIMIT 2000"
    )
    .fetch_all(pool)
    .await?;

    let result = json!({
        "memories": memories.iter().map(|(id, t)| json!({"id": id, "type": t})).collect::<Vec<_>>(),
        "concepts": concepts.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
        "edges": edges.iter().map(|(f, t, r)| json!({"from": f, "to": t, "type": r})).collect::<Vec<_>>(),
    });

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn export_csv(_args: ExportArgs, pool: &SqlitePool) -> Result<()> {
    let edges: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT from_id, to_id, rel_type FROM ontology_edges LIMIT 2000"
    )
    .fetch_all(pool)
    .await?;

    println!("from_id,to_id,relationship_type");
    for (from, to, rel) in edges {
        println!("{},{},{}", from, to, rel);
    }
    Ok(())
}

async fn export_html(_args: ExportArgs, pool: &SqlitePool) -> Result<()> {
    use serde_json::json;
    
    // Get all memories
    let memories: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, type, SUBSTR(content, 1, 100) as preview FROM memories LIMIT 1000"
    )
    .fetch_all(pool)
    .await?;

    // Get all concepts
    let concepts: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, name FROM ontology_concepts LIMIT 500"
    )
    .fetch_all(pool)
    .await?;

    // Get all edges
    let edges: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT from_id, to_id, rel_type FROM ontology_edges LIMIT 2000"
    )
    .fetch_all(pool)
    .await?;

    // Build vis.js nodes array
    let mut nodes_json = Vec::new();
    for (id, mem_type, preview) in &memories {
        let color = match mem_type.as_str() {
            "semantic" => "#ADD8E6",      // light blue
            "episodic" => "#90EE90",      // light green
            "procedural" => "#FFFFE0",    // light yellow
            "conceptual" => "#E0FFFF",    // light cyan
            "contextual" => "#D3D3D3",    // light gray
            _ => "#FFFFFF",
        };
        nodes_json.push(json!({
            "id": &id[..8],
            "label": preview.replace("\"", ""),
            "title": id,
            "color": color,
            "shape": "box",
            "type": "memory"
        }));
    }

    for (id, name) in &concepts {
        nodes_json.push(json!({
            "id": &id[..8],
            "label": name,
            "title": id,
            "color": "#E6D5FA",  // lavender
            "shape": "dot",
            "type": "concept"
        }));
    }

    // Build vis.js edges array
    let mut edges_json = Vec::new();
    for (from, to, rel) in &edges {
        let from_id = if from.len() > 8 { &from[..8] } else { from };
        let to_id = if to.len() > 8 { &to[..8] } else { to };
        edges_json.push(json!({
            "from": from_id,
            "to": to_id,
            "label": rel,
            "arrows": "to",
            "smooth": {"type": "continuous"}
        }));
    }

    let nodes_str = serde_json::to_string(&nodes_json)?;
    let edges_str = serde_json::to_string(&edges_json)?;

    // Generate HTML with vis.js
    let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>voidm Graph Viewer</title>
    <script type="text/javascript" src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
    <style type="text/css">
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        body {{
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            overflow: hidden;
            background: #f5f5f5;
        }}
        #network {{
            width: 100%;
            height: 100vh;
            border: 1px solid #ddd;
            background: white;
        }}
        #controls {{
            position: fixed;
            top: 10px;
            left: 10px;
            background: white;
            padding: 15px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
            z-index: 1000;
            max-width: 300px;
        }}
        .control-group {{
            margin-bottom: 12px;
        }}
        label {{
            display: block;
            margin-bottom: 5px;
            font-weight: 600;
            font-size: 12px;
            color: #333;
        }}
        input[type="text"], select {{
            width: 100%;
            padding: 6px;
            border: 1px solid #ccc;
            border-radius: 4px;
            font-size: 12px;
        }}
        button {{
            background: #007bff;
            color: white;
            border: none;
            padding: 8px 12px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 12px;
            font-weight: 600;
            width: 100%;
        }}
        button:hover {{
            background: #0056b3;
        }}
        #stats {{
            position: fixed;
            bottom: 10px;
            left: 10px;
            background: white;
            padding: 12px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
            z-index: 1000;
            font-size: 12px;
            color: #555;
        }}
        #info {{
            position: fixed;
            top: 10px;
            right: 10px;
            background: white;
            padding: 15px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
            z-index: 1000;
            max-width: 300px;
            max-height: 400px;
            overflow-y: auto;
        }}
        .node-info {{
            font-size: 12px;
            color: #333;
        }}
        .node-info strong {{
            display: block;
            margin-top: 5px;
        }}
        .memory-type {{
            display: inline-block;
            background: #f0f0f0;
            padding: 2px 6px;
            border-radius: 3px;
            font-size: 11px;
            color: #666;
        }}
    </style>
</head>
<body>
    <div id="network"></div>
    
    <div id="controls">
        <div class="control-group">
            <label>Search Node</label>
            <input type="text" id="searchInput" placeholder="Memory or Concept ID...">
        </div>
        
        <div class="control-group">
            <label>Filter Type</label>
            <select id="filterType">
                <option value="">All Nodes</option>
                <option value="memory">Memories Only</option>
                <option value="concept">Concepts Only</option>
            </select>
        </div>
        
        <button onclick="resetView()">Reset View</button>
        <button onclick="focusSelected()">Focus Selected</button>
        <button onclick="downloadPNG()">Download PNG</button>
    </div>
    
    <div id="stats">
        <strong>Nodes:</strong> <span id="nodeCount">0</span><br>
        <strong>Edges:</strong> <span id="edgeCount">0</span><br>
        <strong>Selected:</strong> <span id="selectedCount">0</span>
    </div>
    
    <div id="info">
        <strong>Node Information</strong>
        <div class="node-info" id="nodeInfo">Click a node to see details</div>
    </div>

    <script type="text/javascript">
        // Data
        const rawNodes = {nodes_str};
        const rawEdges = {edges_str};
        
        let allNodes = new vis.DataSet(rawNodes);
        let allEdges = new vis.DataSet(rawEdges);
        let filteredNodes = new vis.DataSet(rawNodes);
        let filteredEdges = new vis.DataSet(rawEdges);

        // Container
        const container = document.getElementById('network');
        const data = {{
            nodes: filteredNodes,
            edges: filteredEdges
        }};

        const options = {{
            physics: {{
                enabled: true,
                stabilization: {{
                    iterations: 200,
                    fit: true
                }},
                barnesHut: {{
                    gravitationalConstant: -26000,
                    centralGravity: 0.3,
                    springLength: 200,
                    springConstant: 0.04
                }}
            }},
            interaction: {{
                navigationButtons: true,
                keyboard: true,
                hover: true
            }},
            layout: {{
                randomSeed: 42
            }}
        }};

        const network = new vis.Network(container, data, options);

        // Update stats
        document.getElementById('nodeCount').textContent = rawNodes.length;
        document.getElementById('edgeCount').textContent = rawEdges.length;

        // Search functionality
        document.getElementById('searchInput').addEventListener('keyup', function() {{
            const query = this.value.toLowerCase();
            const selectedNode = rawNodes.find(n => 
                n.id.toLowerCase().includes(query) || 
                n.label.toLowerCase().includes(query)
            );
            if (selectedNode) {{
                network.selectNodes([selectedNode.id]);
                network.focus(selectedNode.id, {{ scale: 2, animation: {{ duration: 500, easingFunction: 'easeInOutQuad' }} }});
                showNodeInfo(selectedNode);
            }}
        }});

        // Filter functionality
        document.getElementById('filterType').addEventListener('change', function() {{
            const filterType = this.value;
            const filtered = rawNodes.filter(n => !filterType || n.type === filterType);
            const nodeIds = new Set(filtered.map(n => n.id));
            
            filteredNodes.clear();
            filtered.forEach(n => filteredNodes.add(n));
            
            const filteredEdgesList = rawEdges.filter(e => 
                nodeIds.has(e.from) && nodeIds.has(e.to)
            );
            filteredEdges.clear();
            filteredEdgesList.forEach(e => filteredEdges.add(e));
            
            document.getElementById('nodeCount').textContent = filtered.length;
            document.getElementById('edgeCount').textContent = filteredEdgesList.length;
        }});

        // Node selection
        network.on('selectNode', function(params) {{
            if (params.nodes.length > 0) {{
                const nodeId = params.nodes[0];
                const node = rawNodes.find(n => n.id === nodeId);
                showNodeInfo(node);
            }}
        }});

        function showNodeInfo(node) {{
            const info = document.getElementById('nodeInfo');
            if (node) {{
                const type = node.type === 'memory' ? `<span class="memory-type">${{node.type}}</span>` : 'concept';
                info.innerHTML = `
                    <strong>${{node.label}}</strong><br>
                    Type: ${{type}}<br>
                    ID: <code>${{node.title}}</code><br>
                    <small style="color: #999;">Click nodes to highlight</small>
                `;
            }}
        }}

        function resetView() {{
            network.fit({{ animation: {{ duration: 500, easingFunction: 'easeInOutQuad' }} }});
        }}

        function focusSelected() {{
            const selected = network.getSelectedNodes();
            if (selected.length > 0) {{
                network.focus(selected[0], {{ scale: 2, animation: {{ duration: 500, easingFunction: 'easeInOutQuad' }} }});
            }}
        }}

        function downloadPNG() {{
            const canvas = network.canvas.canvas;
            canvas.toBlob(function(blob) {{
                const url = URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = url;
                a.download = 'voidm-graph.png';
                a.click();
                URL.revokeObjectURL(url);
            }});
        }}

        // Keyboard shortcuts
        document.addEventListener('keydown', function(e) {{
            if (e.key === 'Escape') resetView();
        }});
    </script>
</body>
</html>"#, nodes_str = nodes_str, edges_str = edges_str);

    println!("{}", html);
    Ok(())
}

async fn run_show(pool: &SqlitePool) -> Result<()> {
    use std::fs;
    use std::process::Command;
    
    // Generate HTML
    // Capture export_html output to string by reimplementing minimal version
    use serde_json::json;
    
    let memories: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, type, SUBSTR(content, 1, 100) as preview FROM memories LIMIT 1000"
    )
    .fetch_all(pool)
    .await?;

    let concepts: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, name FROM ontology_concepts LIMIT 500"
    )
    .fetch_all(pool)
    .await?;

    let edges: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT from_id, to_id, rel_type FROM ontology_edges LIMIT 2000"
    )
    .fetch_all(pool)
    .await?;

    let mut nodes_json = Vec::new();
    for (id, mem_type, preview) in &memories {
        let color = match mem_type.as_str() {
            "semantic" => "#ADD8E6",
            "episodic" => "#90EE90",
            "procedural" => "#FFFFE0",
            "conceptual" => "#E0FFFF",
            "contextual" => "#D3D3D3",
            _ => "#FFFFFF",
        };
        nodes_json.push(json!({
            "id": &id[..8],
            "label": preview.replace("\"", ""),
            "title": id,
            "color": color,
            "shape": "box",
            "type": "memory"
        }));
    }

    for (id, name) in &concepts {
        nodes_json.push(json!({
            "id": &id[..8],
            "label": name,
            "title": id,
            "color": "#E6D5FA",
            "shape": "dot",
            "type": "concept"
        }));
    }

    let mut edges_json = Vec::new();
    for (from, to, rel) in &edges {
        let from_id = if from.len() > 8 { &from[..8] } else { from };
        let to_id = if to.len() > 8 { &to[..8] } else { to };
        edges_json.push(json!({
            "from": from_id,
            "to": to_id,
            "label": rel,
            "arrows": "to",
            "smooth": {"type": "continuous"}
        }));
    }

    let nodes_str = serde_json::to_string(&nodes_json)?;
    let edges_str = serde_json::to_string(&edges_json)?;
    
    let html = create_html_viewer(&nodes_str, &edges_str);
    
    // Write to temp file
    let temp_path = std::env::temp_dir().join("voidm_graph_viewer.html");
    fs::write(&temp_path, html)?;
    
    // Open in browser
    #[cfg(target_os = "macos")]
    Command::new("open").arg(&temp_path).spawn()?;
    
    #[cfg(target_os = "linux")]
    Command::new("xdg-open").arg(&temp_path).spawn()?;
    
    #[cfg(target_os = "windows")]
    Command::new("cmd").args(&["/C", "start", temp_path.to_str().unwrap()]).spawn()?;
    
    println!("Opened: {}", temp_path.display());
    Ok(())
}

fn create_html_viewer(nodes_str: &str, edges_str: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>voidm Graph Viewer</title>
    <script type="text/javascript" src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
    <style type="text/css">
        *{{ margin:0; padding:0; box-sizing:border-box; }}
        body{{ font-family:Segoe UI, sans-serif; overflow:hidden; background:#f5f5f5; }}
        #network{{ width:100%; height:100vh; border:1px solid #ddd; background:white; }}
        #controls{{ position:fixed; top:10px; left:10px; background:white; padding:15px; border-radius:8px; box-shadow:0 2px 10px rgba(0,0,0,0.1); z-index:1000; max-width:300px; }}
        .control-group{{ margin-bottom:12px; }}
        label{{ display:block; margin-bottom:5px; font-weight:600; font-size:12px; color:#333; }}
        input, select{{ width:100%; padding:6px; border:1px solid #ccc; border-radius:4px; font-size:12px; }}
        button{{ background:#007bff; color:white; border:none; padding:8px 12px; border-radius:4px; cursor:pointer; font-size:12px; font-weight:600; width:100%; margin-bottom:5px; }}
        button:hover{{ background:#0056b3; }}
        #stats{{ position:fixed; bottom:10px; left:10px; background:white; padding:12px; border-radius:8px; box-shadow:0 2px 10px rgba(0,0,0,0.1); z-index:1000; font-size:12px; color:#555; }}
        #info{{ position:fixed; top:10px; right:10px; background:white; padding:15px; border-radius:8px; box-shadow:0 2px 10px rgba(0,0,0,0.1); z-index:1000; max-width:300px; max-height:400px; overflow-y:auto; font-size:12px; }}
    </style>
</head>
<body>
    <div id="network"></div>
    <div id="controls">
        <div class="control-group">
            <label>Search</label>
            <input type="text" id="searchInput" placeholder="Node ID or label...">
        </div>
        <div class="control-group">
            <label>Filter</label>
            <select id="filterType">
                <option value="">All</option>
                <option value="memory">Memories</option>
                <option value="concept">Concepts</option>
            </select>
        </div>
        <button onclick="resetView()">Reset View</button>
        <button onclick="focusSelected()">Focus</button>
    </div>
    <div id="stats">
        <strong>Nodes:</strong> <span id="nodeCount">0</span><br>
        <strong>Edges:</strong> <span id="edgeCount">0</span>
    </div>
    <div id="info">
        <strong>Node Info</strong>
        <div id="nodeInfo">Click a node</div>
    </div>

    <script>
        const rawNodes = {nodes_str};
        const rawEdges = {edges_str};
        let filteredNodes = new vis.DataSet(rawNodes);
        let filteredEdges = new vis.DataSet(rawEdges);
        const data = {{ nodes: filteredNodes, edges: filteredEdges }};
        const options = {{
            physics: {{ enabled: true, stabilization: {{ iterations: 200 }} }},
            interaction: {{ navigationButtons: true, keyboard: true }},
            layout: {{ randomSeed: 42 }}
        }};
        const network = new vis.Network(document.getElementById('network'), data, options);
        document.getElementById('nodeCount').textContent = rawNodes.length;
        document.getElementById('edgeCount').textContent = rawEdges.length;
        
        document.getElementById('searchInput').addEventListener('keyup', function() {{
            const query = this.value.toLowerCase();
            const node = rawNodes.find(n => n.id.includes(query) || n.label.toLowerCase().includes(query));
            if (node) {{ network.selectNodes([node.id]); network.focus(node.id, {{ scale: 2 }}); }}
        }});
        
        document.getElementById('filterType').addEventListener('change', function() {{
            const filtered = rawNodes.filter(n => !this.value || n.type === this.value);
            const ids = new Set(filtered.map(n => n.id));
            filteredNodes.clear(); filtered.forEach(n => filteredNodes.add(n));
            const fedges = rawEdges.filter(e => ids.has(e.from) && ids.has(e.to));
            filteredEdges.clear(); fedges.forEach(e => filteredEdges.add(e));
            document.getElementById('nodeCount').textContent = filtered.length;
            document.getElementById('edgeCount').textContent = fedges.length;
        }});
        
        network.on('selectNode', function(p) {{
            if (p.nodes.length) {{
                const node = rawNodes.find(n => n.id === p.nodes[0]);
                document.getElementById('nodeInfo').innerHTML = `<strong>${{node.label}}</strong><br>ID: <code>${{node.title}}</code>`;
            }}
        }});
        
        function resetView() {{ network.fit(); }}
        function focusSelected() {{ const s = network.getSelectedNodes(); if (s.length) network.focus(s[0], {{ scale: 2 }}); }}
        document.addEventListener('keydown', e => {{ if (e.key === 'Escape') resetView(); }});
    </script>
</body>
</html>"#, nodes_str = nodes_str, edges_str = edges_str)
}
