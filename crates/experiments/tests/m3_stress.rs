use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use mpl_experiments::{load_spec, run_experiment, write_outputs};

#[derive(Clone, Debug, PartialEq)]
struct Metrics {
    n_vertices: usize,
    n_edges_undirected: usize,
    triangles: u64,
    clustering_num: u64,
    clustering_den: u64,
    beta1_est: i64,
    components: usize,
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

    fn intersection_count(&self, other: &Self) -> u64 {
        self.blocks
            .iter()
            .zip(other.blocks.iter())
            .map(|(a, b)| (a & b).count_ones() as u64)
            .sum()
    }
}

#[derive(Clone, Debug)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.state >> 32) as u32
    }
}

fn threshold(p_num: u32, p_den: u32) -> u64 {
    let denom = p_den as u64;
    let scale = u32::MAX as u64 + 1;
    (p_num as u64) * scale / denom
}

fn build_metrics(n: usize, seed: u64, p_num: u32, p_den: u32) -> Metrics {
    let mut rng = Lcg::new(seed);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut adj_list: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut neighbors_hi: Vec<BitSet> = (0..n).map(|_| BitSet::new(n)).collect();
    let thresh = threshold(p_num, p_den);

    for u in 0..n {
        for v in (u + 1)..n {
            if (rng.next_u32() as u64) < thresh {
                edges.push((u, v));
                adj_list[u].push(v);
                adj_list[v].push(u);
                neighbors_hi[u].set(v);
            }
        }
    }

    for neighbors in &mut adj_list {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    let mut triangles = 0u64;
    for (u, v) in &edges {
        triangles += neighbors_hi[*u].intersection_count(&neighbors_hi[*v]);
    }

    let mut clustering_den = 0u64;
    for neighbors in &adj_list {
        let deg = neighbors.len() as u64;
        if deg >= 2 {
            clustering_den += deg * (deg - 1) / 2;
        }
    }
    let clustering_num = if clustering_den == 0 {
        0
    } else {
        triangles.saturating_mul(3)
    };

    let components = connected_components_undirected(&adj_list);
    let beta1_est = edges.len() as i64 - n as i64 + components as i64;

    Metrics {
        n_vertices: n,
        n_edges_undirected: edges.len(),
        triangles,
        clustering_num,
        clustering_den,
        beta1_est,
        components,
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

#[ignore]
#[test]
fn stress_triangles_bitset_dense_n1024() {
    let n = 1024;
    let seed = 0xC0FFEEu64;
    let p_num = 20;
    let p_den = 100;
    let m1 = build_metrics(n, seed, p_num, p_den);
    let m2 = build_metrics(n, seed, p_num, p_den);
    assert_eq!(m1, m2);
    assert!(m1.n_edges_undirected <= n * (n - 1) / 2);
    assert!(m1.triangles <= (n as u64) * ((n - 1) as u64) * ((n - 2) as u64) / 6);
    if m1.clustering_den == 0 {
        assert_eq!(m1.clustering_num, 0);
    } else {
        assert_eq!(m1.clustering_num, m1.triangles * 3);
    }
    println!(
        "n={n} p={:.2} edges={} triangles={} clustering_den={}",
        p_num as f64 / p_den as f64,
        m1.n_edges_undirected,
        m1.triangles,
        m1.clustering_den
    );
}

struct CsvTable {
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn read_csv_table(path: &Path) -> CsvTable {
    let content = fs::read_to_string(path).expect("read csv");
    let mut lines = content.lines();
    let header_line = lines.next().expect("missing header");
    let header = parse_csv_line(header_line);
    let mut rows = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let row = parse_csv_line(line);
        assert_eq!(row.len(), header.len(), "row length mismatch");
        rows.push(row);
    }
    CsvTable { header, rows }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(ch);
            }
        } else {
            match ch {
                '"' => in_quotes = true,
                ',' => {
                    fields.push(current);
                    current = String::new();
                }
                _ => current.push(ch),
            }
        }
    }
    fields.push(current);
    fields
}

fn column_index(header: &[String], name: &str) -> usize {
    header
        .iter()
        .position(|h| h == name)
        .unwrap_or_else(|| panic!("missing column: {name}"))
}

fn row_by_weight(table: &CsvTable, weight: u64) -> &[String] {
    let idx = column_index(&table.header, "weight");
    table
        .rows
        .iter()
        .find(|row| parse_u64(&row[idx]) == weight)
        .unwrap_or_else(|| panic!("missing weight {weight}"))
}

fn value<'a>(table: &'a CsvTable, row: &'a [String], name: &str) -> &'a str {
    let idx = column_index(&table.header, name);
    &row[idx]
}

fn parse_u64(value: &str) -> u64 {
    value.parse().expect("parse u64")
}

fn prepare_dir(name: &str) -> PathBuf {
    let path = PathBuf::from("target/tmp").join(name);
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create tmp dir");
    path
}

fn name_width(m: usize) -> usize {
    let max_idx = m.saturating_sub(1);
    max_idx.to_string().len().max(1)
}

fn format_name(idx: usize, width: usize) -> String {
    format!("l{:0width$}", idx, width = width)
}

fn build_random_spec(
    m: usize,
    w: usize,
    p_num: u32,
    p_den: u32,
    seed: u64,
    out_dir: &str,
) -> String {
    let mut rng = Lcg::new(seed);
    let width = name_width(m);
    let mut names = Vec::with_capacity(m);
    for i in 0..m {
        names.push(format_name(i, width));
    }

    let mut out = String::new();
    out.push_str("[experiment]\n");
    out.push_str(&format!(
        "id = \"stress_m{m}_w{w}_p{p_num}_{p_den}_seed{seed}\"\n"
    ));
    out.push_str(&format!("out_dir = \"{out_dir}\"\n"));
    out.push_str(&format!("w_min = {w}\n"));
    out.push_str(&format!("w_max = {w}\n\n"));

    out.push_str("[alphabet]\n");
    out.push_str("vars = [\"x\", \"y\"]\n\n");

    for name in &names {
        let a = 1 + (rng.next_u32() % 9) as i32;
        let b = 1 + (rng.next_u32() % 9) as i32;
        out.push_str("[[alphabet.letters]]\n");
        out.push_str(&format!("name = \"{name}\"\n"));
        out.push_str(&format!(
            "expr = \"(+ 1 (* {a} (^ x 2)) (* {b} (^ y 2)))\"\n\n"
        ));
    }

    let mut pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
    for i in 0..m {
        pairs.insert((i, (i + 1) % m));
    }
    let thresh = threshold(p_num, p_den);
    for a in 0..m {
        for b in 0..m {
            if a == b || pairs.contains(&(a, b)) {
                continue;
            }
            if (rng.next_u32() as u64) < thresh {
                pairs.insert((a, b));
            }
        }
    }

    out.push_str("[constraints]\n");
    out.push_str("adjacency_mode = \"allow\"\n");
    out.push_str("adjacency_pairs = [\n");
    let pairs_vec: Vec<(usize, usize)> = pairs.into_iter().collect();
    for (idx, (a, b)) in pairs_vec.iter().enumerate() {
        out.push_str(&format!("  [\"{}\", \"{}\"]", names[*a], names[*b]));
        if idx + 1 != pairs_vec.len() {
            out.push(',');
        }
        if (idx + 1) % 4 == 0 || idx + 1 == pairs_vec.len() {
            out.push('\n');
        } else {
            out.push(' ');
        }
    }
    out.push_str("]\n\n");

    out.push_str("[constraints.budget]\n");
    out.push_str(&format!("max_states = {m}\n\n"));

    out.push_str("[pairs]\n");
    out.push_str("count_mode = \"active_word_positions\"\n");
    out
}

fn undirected_metrics_from_allowed_pairs(pairs: &[Vec<bool>]) -> (usize, u64, u64) {
    let n = pairs.len();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut adj_list: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut neighbors_hi: Vec<BitSet> = (0..n).map(|_| BitSet::new(n)).collect();

    for u in 0..n {
        for v in (u + 1)..n {
            if pairs[u][v] || pairs[v][u] {
                edges.push((u, v));
                adj_list[u].push(v);
                adj_list[v].push(u);
                neighbors_hi[u].set(v);
            }
        }
    }

    for neighbors in &mut adj_list {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    let mut triangles = 0u64;
    for (u, v) in &edges {
        triangles += neighbors_hi[*u].intersection_count(&neighbors_hi[*v]);
    }

    let mut clustering_den = 0u64;
    for neighbors in &adj_list {
        let deg = neighbors.len() as u64;
        if deg >= 2 {
            clustering_den += deg * (deg - 1) / 2;
        }
    }

    (edges.len(), triangles, clustering_den)
}

#[ignore]
#[test]
fn stress_e2e_random_suite_w5() {
    const HEADER: &str = "weight,status,error_code,error,n_vertices,n_edges_undirected,triangles,clustering_num,clustering_den,beta1_est,triplets_supported_by_triangles_num,triplets_supported_by_triangles_den";
    let m = 64;
    let w = 5;
    let seeds = [1u64, 2u64];
    let p_values = [(6u32, 100u32), (12u32, 100u32)];

    for seed in seeds {
        for (p_num, p_den) in p_values {
            let p_label = format!("{:.2}", p_num as f64 / p_den as f64);
            let id = format!("m3_stress_m{m}_w{w}_p{p_label}_seed{seed}");
            let out_dir = format!("target/tmp/{id}");
            let spec_contents = build_random_spec(m, w, p_num, p_den, seed, &out_dir);
            let spec_dir = prepare_dir(&format!("{id}_spec"));
            let spec_path = spec_dir.join("spec.toml");
            fs::write(&spec_path, spec_contents).expect("write spec");

            let mut cfg = load_spec(&spec_path).expect("load spec");
            cfg.out_dir = PathBuf::from(&out_dir);
            let allowed_pairs = cfg
                .constraints
                .allowed_pairs
                .as_ref()
                .expect("allowed_pairs");
            let (edges, triangles, clustering_den) =
                undirected_metrics_from_allowed_pairs(allowed_pairs);
            let report = run_experiment(&cfg).expect("run experiment");
            write_outputs(&report, &cfg.out_dir).expect("write outputs");

            let csv_path = cfg.out_dir.join("skeleton2_metrics.csv");
            let table = read_csv_table(&csv_path);
            let header = table.header.join(",");
            assert_eq!(header, HEADER);

            let row = row_by_weight(&table, w as u64);
            let _ = parse_u64(value(&table, row, "n_edges_undirected"));
            let _ = parse_u64(value(&table, row, "triangles"));
            let _ = parse_u64(value(&table, row, "clustering_den"));
            println!(
                "m={m} p={p_label} seed={seed} edges={edges} triangles={triangles} clustering_den={clustering_den}"
            );
        }
    }
}
