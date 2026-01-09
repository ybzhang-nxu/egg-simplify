use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skeleton2Metrics {
    pub n_vertices: usize,
    pub n_edges_undirected: usize,
    pub triangles: u64,
    pub clustering_num: u64,
    pub clustering_den: u64,
    pub beta1_est: i64,
    pub triplets_supported_by_triangles_num: u64,
    pub triplets_supported_by_triangles_den: u64,
}

#[derive(Clone, Debug)]
struct BitSet {
    blocks: Vec<u64>,
}

impl BitSet {
    fn new(len: usize) -> Self {
        let blocks = len.div_ceil(64);
        Self {
            blocks: vec![0; blocks],
        }
    }

    fn set(&mut self, idx: usize) {
        let block = idx / 64;
        let bit = idx % 64;
        if let Some(slot) = self.blocks.get_mut(block) {
            *slot |= 1u64 << bit;
        }
    }

    fn contains(&self, idx: usize) -> bool {
        let block = idx / 64;
        let bit = idx % 64;
        self.blocks
            .get(block)
            .is_some_and(|slot| (slot & (1u64 << bit)) != 0)
    }

    fn intersection_count(&self, other: &Self) -> u64 {
        self.blocks
            .iter()
            .zip(other.blocks.iter())
            .map(|(a, b)| (a & b).count_ones() as u64)
            .sum()
    }
}

pub(crate) fn compute_skeleton2_metrics(
    n_vertices: usize,
    pair_counts: &std::collections::BTreeMap<(usize, usize), u64>,
    triplet_counts: &std::collections::BTreeMap<(usize, usize, usize), u64>,
) -> Skeleton2Metrics {
    // Build an undirected simple graph from directed pair counts:
    // - vertices are the same letter ids used by topology_metrics (0..n_vertices)
    // - an undirected edge exists if (u->v) or (v->u) appears at least once
    // - self-loops are ignored
    let mut undirected_edges = BTreeSet::new();
    for &(a, b) in pair_counts.keys() {
        if a >= n_vertices || b >= n_vertices || a == b {
            continue;
        }
        let (u, v) = if a < b { (a, b) } else { (b, a) };
        undirected_edges.insert((u, v));
    }

    let edges: Vec<(usize, usize)> = undirected_edges.into_iter().collect();
    let n_edges_undirected = edges.len();

    let mut adj_list: Vec<Vec<usize>> = vec![Vec::new(); n_vertices];
    let mut adj_bits: Vec<BitSet> = (0..n_vertices).map(|_| BitSet::new(n_vertices)).collect();
    let mut neighbors_hi: Vec<BitSet> = (0..n_vertices).map(|_| BitSet::new(n_vertices)).collect();

    for (u, v) in &edges {
        adj_list[*u].push(*v);
        adj_list[*v].push(*u);
        adj_bits[*u].set(*v);
        adj_bits[*v].set(*u);
        // Higher-neighbor orientation: neighbors_hi[u] contains only v > u.
        neighbors_hi[*u].set(*v);
    }

    for neighbors in &mut adj_list {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    // Triangle count using bitset intersections:
    // triangles = sum_{u<v} popcount(neighbors_hi[u] & neighbors_hi[v])
    let mut triangles = 0u64;
    for (u, v) in &edges {
        triangles += neighbors_hi[*u].intersection_count(&neighbors_hi[*v]);
    }

    // Global transitivity (clustering): 3 * triangles / sum_v choose(deg(v), 2).
    let mut clustering_den = 0u64;
    for neighbors in &adj_list {
        let deg = neighbors.len() as u64;
        if deg >= 2 {
            clustering_den += deg * (deg - 1) / 2;
        }
    }
    let (clustering_num, clustering_den) = if clustering_den == 0 {
        (0, 0)
    } else {
        (triangles.saturating_mul(3), clustering_den)
    };

    let components = connected_components_undirected(&adj_list);
    let beta1_est = n_edges_undirected as i64 - n_vertices as i64 + components as i64;

    // Triplet support: count ordered triplets whose distinct vertices form a triangle.
    let triplets_supported_by_triangles_den: u64 = triplet_counts.values().sum();
    let mut triplets_supported_by_triangles_num = 0u64;
    if triplets_supported_by_triangles_den > 0 {
        for (&(a, b, c), count) in triplet_counts {
            if a >= n_vertices || b >= n_vertices || c >= n_vertices {
                continue;
            }
            if a == b || a == c || b == c {
                continue;
            }
            if adj_bits[a].contains(b) && adj_bits[a].contains(c) && adj_bits[b].contains(c) {
                triplets_supported_by_triangles_num += *count;
            }
        }
    }
    if triplets_supported_by_triangles_den == 0 {
        triplets_supported_by_triangles_num = 0;
    }

    Skeleton2Metrics {
        n_vertices,
        n_edges_undirected,
        triangles,
        clustering_num,
        clustering_den,
        beta1_est,
        triplets_supported_by_triangles_num,
        triplets_supported_by_triangles_den: if triplets_supported_by_triangles_den == 0 {
            0
        } else {
            triplets_supported_by_triangles_den
        },
    }
}

fn connected_components_undirected(adj: &[Vec<usize>]) -> usize {
    if adj.is_empty() {
        return 0;
    }
    let mut visited = vec![false; adj.len()];
    let mut count = 0;
    for v in 0..adj.len() {
        if visited[v] {
            continue;
        }
        count += 1;
        let mut stack = vec![v];
        visited[v] = true;
        while let Some(node) = stack.pop() {
            for &next in &adj[node] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
    }
    count
}
