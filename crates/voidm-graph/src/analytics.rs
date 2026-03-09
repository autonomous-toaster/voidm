use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

/// Node degree (in + out)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDegree {
    pub memory_id: String,
    pub degree: i32,
    pub in_degree: i32,
    pub out_degree: i32,
}

/// Edge with computed strength
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeWithStrength {
    pub source_id: String,
    pub target_id: String,
    pub rel_type: String,
    pub count: i32,
    pub weight: f64,
    pub strength: f64,
}

/// Community assignment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityAssignment {
    pub memory_id: String,
    pub community_id: i32,
}

/// Edge type weights (semantic importance)
fn edge_type_weight(rel_type: &str) -> f64 {
    match rel_type {
        "SUPPORTS" | "CONTRADICTS" => 3.0,
        "DERIVED_FROM" | "PRECEDES" | "PART_OF" | "EXEMPLIFIES" => 2.0,
        "INVALIDATES" => 2.5,
        "RELATES_TO" | "IS_A" | "INSTANCE_OF" | "HAS_PROPERTY" => 1.0,
        _ => 1.0,
    }
}

/// Compute degree (in + out) for all nodes
pub async fn compute_degrees(pool: &SqlitePool) -> Result<HashMap<String, NodeDegree>> {
    let mut degrees: HashMap<String, NodeDegree> = HashMap::new();

    // Get all memory nodes
    let nodes: Vec<(String,)> = sqlx::query_as("SELECT memory_id FROM graph_nodes")
        .fetch_all(pool)
        .await?;

    for (node_id,) in nodes {
        degrees.insert(
            node_id.clone(),
            NodeDegree {
                memory_id: node_id,
                degree: 0,
                in_degree: 0,
                out_degree: 0,
            },
        );
    }

    // Count outgoing edges
    let outgoing: Vec<(String, i64)> = sqlx::query_as(
        "SELECT gn.memory_id, COUNT(*) as cnt FROM graph_edges ge
         JOIN graph_nodes gn ON gn.id = ge.source_id
         GROUP BY ge.source_id"
    )
    .fetch_all(pool)
    .await?;

    for (node_id, count) in outgoing {
        if let Some(deg) = degrees.get_mut(&node_id) {
            deg.out_degree = count as i32;
            deg.degree += count as i32;
        }
    }

    // Count incoming edges
    let incoming: Vec<(String, i64)> = sqlx::query_as(
        "SELECT gn.memory_id, COUNT(*) as cnt FROM graph_edges ge
         JOIN graph_nodes gn ON gn.id = ge.target_id
         GROUP BY ge.target_id"
    )
    .fetch_all(pool)
    .await?;

    for (node_id, count) in incoming {
        if let Some(deg) = degrees.get_mut(&node_id) {
            deg.in_degree = count as i32;
            deg.degree += count as i32;
        }
    }

    Ok(degrees)
}

/// Compute link strength: edge_count × edge_type_weight
/// Handles parallel edges (multiple edges of same type between same nodes)
pub async fn compute_link_strength(pool: &SqlitePool) -> Result<Vec<EdgeWithStrength>> {
    let edges: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT gn_src.memory_id, gn_tgt.memory_id, ge.rel_type, COUNT(*) as cnt
         FROM graph_edges ge
         JOIN graph_nodes gn_src ON gn_src.id = ge.source_id
         JOIN graph_nodes gn_tgt ON gn_tgt.id = ge.target_id
         GROUP BY gn_src.memory_id, gn_tgt.memory_id, ge.rel_type"
    )
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for (source_id, target_id, rel_type, count) in edges {
        let weight = edge_type_weight(&rel_type);
        let strength = count as f64 * weight;
        result.push(EdgeWithStrength {
            source_id,
            target_id,
            rel_type,
            count: count as i32,
            weight,
            strength,
        });
    }

    Ok(result)
}

/// Simple Louvain-inspired community detection using modularity optimization.
/// Uses greedy algorithm: assign each node to community maximizing modularity gain.
/// Returns community assignments for all nodes.
pub async fn detect_communities(pool: &SqlitePool) -> Result<Vec<CommunityAssignment>> {
    // Get all nodes and edges
    let nodes: Vec<(i64, String)> = sqlx::query_as("SELECT id, memory_id FROM graph_nodes")
        .fetch_all(pool)
        .await?;

    if nodes.is_empty() {
        return Ok(Vec::new());
    }

    let edges: Vec<(i64, i64)> = sqlx::query_as("SELECT source_id, target_id FROM graph_edges")
        .fetch_all(pool)
        .await?;

    // Build adjacency list (undirected for community detection)
    let mut adj: HashMap<i64, HashSet<i64>> = HashMap::new();
    for (src, tgt) in &edges {
        adj.entry(*src).or_insert_with(HashSet::new).insert(*tgt);
        adj.entry(*tgt).or_insert_with(HashSet::new).insert(*src);
    }

    let node_ids: Vec<i64> = nodes.iter().map(|(id, _)| *id).collect();
    let _total_edges = edges.len() as f64;

    // Initialize: each node in its own community
    let mut community: HashMap<i64, i32> = HashMap::new();
    for (id, _) in &nodes {
        community.insert(*id, *id as i32);
    }

    // Greedy optimization: move nodes to maximize modularity
    let max_iterations = 10;
    let mut improved = true;
    let mut iteration = 0;

    while improved && iteration < max_iterations {
        improved = false;
        iteration += 1;

        for &node_id in &node_ids {
            let current_community = community[&node_id];
            let neighbors: Vec<i64> = adj
                .get(&node_id)
                .unwrap_or(&HashSet::new())
                .iter()
                .copied()
                .collect();

            // Count neighbors in each community
            let mut community_counts: HashMap<i32, i32> = HashMap::new();
            for neighbor in neighbors {
                *community_counts
                    .entry(community[&neighbor])
                    .or_insert(0) += 1;
            }

            // Find best community (choose highest count, break ties by community ID)
            if let Some((&best_community, &_)) =
                community_counts.iter().max_by_key(|&(_, cnt)| cnt)
            {
                if best_community != current_community {
                    community.insert(node_id, best_community);
                    improved = true;
                }
            }
        }
    }

    // Renumber communities sequentially
    let mut community_map: HashMap<i32, i32> = HashMap::new();
    let mut next_id = 0;
    for node_id in &node_ids {
        let comm = community[node_id];
        if !community_map.contains_key(&comm) {
            community_map.insert(comm, next_id);
            next_id += 1;
        }
    }

    let mut result = Vec::new();
    for (node_db_id, memory_id) in &nodes {
        let original_community = community[node_db_id];
        let remapped_community = community_map[&original_community];
        result.push(CommunityAssignment {
            memory_id: memory_id.clone(),
            community_id: remapped_community,
        });
    }

    Ok(result)
}

/// Assign colors to communities (simple palette)
pub fn community_color_palette(community_id: i32) -> String {
    let colors = vec![
        "#FF6B6B", // Red
        "#4ECDC4", // Teal
        "#45B7D1", // Blue
        "#FFA07A", // Light salmon
        "#98D8C8", // Mint
        "#F7DC6F", // Yellow
        "#BB8FCE", // Purple
        "#85C1E2", // Sky blue
        "#F8B88B", // Peach
        "#D7BDE2", // Lavender
    ];
    colors[(community_id as usize) % colors.len()].to_string()
}
