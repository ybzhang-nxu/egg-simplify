use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mpl_experiments::{parse_spec_str, run_experiment, write_outputs, ExperimentConfig};

const L1_A2_GOLDEN_TOML: &str = r#"
[experiment]
id = "L1_A2_cluster_golden"
title = "A2 pentagon adjacency golden (w<=6)"
out_dir = "OUT_DIR_REPLACED_IN_TEST"
w_min = 1
w_max = 6

[alphabet]
vars = ["x1", "x2"]

[[alphabet.letters]]
name = "x1"
expr = "x1"

[[alphabet.letters]]
name = "x2"
expr = "x2"

[[alphabet.letters]]
name = "x3"
expr = "(/ (+ 1 x2) x1)"

[[alphabet.letters]]
name = "x4"
expr = "(/ (+ 1 x1 x2) x1 x2)"

[[alphabet.letters]]
name = "x5"
expr = "(/ (+ 1 x1) x2)"

[constraints]
first_entry = ["x1", "x2"]
adjacency_mode = "allow"
adjacency_pairs = [
  ["x1","x2"], ["x2","x1"],
  ["x2","x3"], ["x3","x2"],
  ["x3","x4"], ["x4","x3"],
  ["x4","x5"], ["x5","x4"],
  ["x5","x1"], ["x1","x5"],
]

[pairs]
count_mode = "active_word_positions"
"#;

fn golden_config(out_dir: &Path) -> ExperimentConfig {
    let mut cfg = parse_spec_str(L1_A2_GOLDEN_TOML).expect("parse spec");
    cfg.out_dir = out_dir.to_path_buf();
    cfg
}

fn prepare_dir(name: &str) -> PathBuf {
    let path = PathBuf::from("target/tmp").join(name);
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create tmp dir");
    path
}

fn run_and_write(out_dir: &Path) {
    let cfg = golden_config(out_dir);
    let report = run_experiment(&cfg).expect("run experiment");
    write_outputs(&report, out_dir).expect("write outputs");
}

#[test]
fn golden_l1_a2_dim_rank_stats() {
    let out_dir = prepare_dir("m1_golden_l1_dim");
    run_and_write(&out_dir);

    let table = read_csv_table(&out_dir.join("dim_vs_w.csv"));
    let expected = [
        (
            1_u64, 2_u64, 2_u64, 0_u64, 0_u64, 0_u64, 0_u64, 0_u64, 0_u64,
        ),
        (2, 4, 3, 1, 5, 1, 5, 5, 4),
        (3, 8, 2, 6, 35, 6, 35, 5, 2),
        (4, 16, 2, 14, 110, 14, 110, 5, 2),
        (5, 32, 2, 30, 300, 30, 300, 5, 2),
        (6, 64, 2, 62, 760, 62, 760, 5, 2),
    ];

    for (
        weight,
        n_words,
        dim,
        rank,
        rows_attempted,
        rows_inserted,
        samples_used,
        envs_total,
        max_row_nnz,
    ) in expected
    {
        let row = row_by_weight(&table, weight);
        assert_eq!(parse_u64(value(&table, row, "n_words_allowed")), n_words);
        assert_eq!(parse_u64(value(&table, row, "dim")), dim);
        assert_eq!(parse_u64(value(&table, row, "rank")), rank);
        assert_eq!(rank + dim, n_words);
        let actual_rows_attempted = parse_u64(value(&table, row, "rows_attempted"));
        assert_eq!(actual_rows_attempted, rows_attempted);
        assert_eq!(
            parse_u64(value(&table, row, "rows_inserted")),
            rows_inserted
        );
        let actual_samples_used = parse_u64(value(&table, row, "samples_used"));
        assert_eq!(actual_samples_used, samples_used);
        let actual_envs_total = parse_u64(value(&table, row, "envs_total"));
        assert_eq!(actual_envs_total, envs_total);
        assert_eq!(parse_u64(value(&table, row, "max_row_nnz")), max_row_nnz);
        assert_eq!(parse_u64(value(&table, row, "rows_skipped_singular")), 0);
        assert_eq!(
            parse_u64(value(&table, row, "constraints_insufficient_samples")),
            0
        );
        assert_eq!(value(&table, row, "status"), "ok");
        assert!(value(&table, row, "error_code").is_empty());

        if weight >= 2 {
            assert_eq!(actual_rows_attempted % actual_envs_total, 0);
            assert_eq!(actual_samples_used, actual_rows_attempted);
        }
    }

    // Golden regression: rows_attempted is extremely sensitive to the
    // (context grouping × position k × variable-pair) constraint generation.
    // For vars=2 there is only one variable pair, so rows_attempted / envs_total
    // equals the total number of distinct contexts across all k.
    // For this L1 A2 spec and the fixed env table (envs_total=5), we expect:
    //
    // w=2..6: rows_attempted = [5, 35, 110, 300, 760]
    //
    // This case has no singular samples, so samples_used == rows_attempted.
    let expected_rows_attempted = [(2_u64, 5_u64), (3, 35), (4, 110), (5, 300), (6, 760)];

    for (weight, rows_attempted) in expected_rows_attempted {
        let row = row_by_weight(&table, weight);
        let actual_rows_attempted = parse_u64(value(&table, row, "rows_attempted"));
        let actual_envs_total = parse_u64(value(&table, row, "envs_total"));
        let actual_samples_used = parse_u64(value(&table, row, "samples_used"));
        let rows_skipped_singular = parse_u64(value(&table, row, "rows_skipped_singular"));
        let constraints_insufficient_samples =
            parse_u64(value(&table, row, "constraints_insufficient_samples"));

        assert_eq!(actual_rows_attempted, rows_attempted);
        assert!(actual_envs_total > 0);
        assert_eq!(actual_rows_attempted % actual_envs_total, 0);
        assert_eq!(rows_skipped_singular, 0);
        assert_eq!(constraints_insufficient_samples, 0);
        assert_eq!(actual_samples_used, actual_rows_attempted);
    }
}

#[test]
fn golden_l1_a2_pairs_by_weight_and_aggregate() {
    let out_dir = prepare_dir("m1_golden_l1_pairs");
    run_and_write(&out_dir);

    let pairs_by_weight = read_pairs_by_weight(&out_dir.join("pairs_by_weight.csv"));
    let pairs_total = read_pairs(&out_dir.join("pairs.csv"));

    assert_eq!(edges_for_weight(&pairs_by_weight, 2).len(), 4);
    for w in 3..=6 {
        assert_eq!(edges_for_weight(&pairs_by_weight, w).len(), 6);
    }

    let expected_edges_w3 = BTreeSet::from([
        ("x1".to_string(), "x2".to_string()),
        ("x1".to_string(), "x5".to_string()),
        ("x2".to_string(), "x1".to_string()),
        ("x2".to_string(), "x3".to_string()),
        ("x3".to_string(), "x2".to_string()),
        ("x5".to_string(), "x1".to_string()),
    ]);
    assert_eq!(edges_for_weight(&pairs_by_weight, 3), expected_edges_w3);

    assert_eq!(pair_count(&pairs_by_weight, 2, "x1", "x2"), 1);
    assert_eq!(pair_count(&pairs_by_weight, 3, "x1", "x2"), 2);
    assert_eq!(pair_count(&pairs_by_weight, 4, "x1", "x2"), 6);
    assert_eq!(pair_count(&pairs_by_weight, 5, "x1", "x2"), 8);
    assert_eq!(pair_count(&pairs_by_weight, 6, "x1", "x2"), 20);

    let aggregated = aggregate_pairs(&pairs_by_weight);
    assert_eq!(pairs_total, aggregated);
}

#[test]
fn golden_l1_a2_topology_metrics() {
    let out_dir = prepare_dir("m1_golden_l1_topology");
    run_and_write(&out_dir);

    let table = read_csv_table(&out_dir.join("topology_metrics.csv"));

    for w in 1..=6 {
        let row = row_by_weight(&table, w);
        assert_eq!(parse_u64(value(&table, row, "n_vertices")), 5);
        assert_eq!(value(&table, row, "status"), "ok");
        assert!(value(&table, row, "error_code").is_empty());
    }

    let row_w1 = row_by_weight(&table, 1);
    assert_eq!(parse_u64(value(&table, row_w1, "n_edges")), 0);
    assert_eq!(
        parse_u64(value(&table, row_w1, "strongly_connected_components")),
        5
    );

    let row_w2 = row_by_weight(&table, 2);
    assert_eq!(parse_u64(value(&table, row_w2, "n_edges")), 4);
    assert_eq!(
        parse_u64(value(&table, row_w2, "strongly_connected_components")),
        4
    );
    assert_eq!(parse_u64(value(&table, row_w2, "density_num")), 4);
    assert_eq!(parse_u64(value(&table, row_w2, "density_den")), 25);
    assert_eq!(parse_u64(value(&table, row_w2, "avg_out_degree_num")), 4);
    assert_eq!(parse_u64(value(&table, row_w2, "avg_out_degree_den")), 5);

    for w in 3..=6 {
        let row = row_by_weight(&table, w);
        assert_eq!(parse_u64(value(&table, row, "n_edges")), 6);
        assert_eq!(
            parse_u64(value(&table, row, "strongly_connected_components")),
            2
        );
        assert_eq!(parse_u64(value(&table, row, "density_num")), 6);
        assert_eq!(parse_u64(value(&table, row, "density_den")), 25);
        assert_eq!(parse_u64(value(&table, row, "avg_out_degree_num")), 6);
        assert_eq!(parse_u64(value(&table, row, "avg_out_degree_den")), 5);
        assert_eq!(parse_u64(value(&table, row, "max_out_degree")), 2);
    }
}

#[test]
fn determinism_l1_a2_outputs_are_identical() {
    let out_dir_1 = prepare_dir("m1_det_l1_a2_1");
    let out_dir_2 = prepare_dir("m1_det_l1_a2_2");

    run_and_write(&out_dir_1);
    run_and_write(&out_dir_2);

    for name in [
        "basis_stats.txt",
        "dim_vs_w.csv",
        "pairs.csv",
        "pairs_by_weight.csv",
        "topology_metrics.csv",
    ] {
        let b1 = fs::read(out_dir_1.join(name)).expect("read output 1");
        let b2 = fs::read(out_dir_2.join(name)).expect("read output 2");
        assert_eq!(b1, b2, "file differs: {name}");
    }
}

#[test]
fn adjacency_allow_requires_pairs() {
    let spec = r#"
[experiment]
id = "adj_empty"
out_dir = "unused"
w_min = 1
w_max = 1

[alphabet]
vars = ["x1"]

[[alphabet.letters]]
name = "x1"
expr = "x1"

[constraints]
adjacency_mode = "allow"
adjacency_pairs = []
"#;
    let err = parse_spec_str(spec).expect_err("expected error");
    let msg = err.to_string();
    assert!(msg.contains("adjacency_mode=allow"));
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

fn row_by_weight<'a>(table: &'a CsvTable, weight: u64) -> &'a Vec<String> {
    let idx = column_index(&table.header, "weight");
    table
        .rows
        .iter()
        .find(|row| parse_u64(&row[idx]) == weight)
        .unwrap_or_else(|| panic!("missing weight {weight}"))
}

fn value<'a>(table: &'a CsvTable, row: &'a Vec<String>, name: &str) -> &'a str {
    let idx = column_index(&table.header, name);
    &row[idx]
}

fn parse_u64(value: &str) -> u64 {
    value.parse().expect("parse u64")
}

fn read_pairs_by_weight(path: &Path) -> BTreeMap<u64, BTreeMap<(String, String), u64>> {
    let table = read_csv_table(path);
    let weight_idx = column_index(&table.header, "weight");
    let a_idx = column_index(&table.header, "a");
    let b_idx = column_index(&table.header, "b");
    let count_idx = column_index(&table.header, "count");
    let mut out: BTreeMap<u64, BTreeMap<(String, String), u64>> = BTreeMap::new();
    for row in table.rows {
        let weight = parse_u64(&row[weight_idx]);
        let a = row[a_idx].clone();
        let b = row[b_idx].clone();
        let count = parse_u64(&row[count_idx]);
        out.entry(weight).or_default().insert((a, b), count);
    }
    out
}

fn read_pairs(path: &Path) -> BTreeMap<(String, String), u64> {
    let table = read_csv_table(path);
    let a_idx = column_index(&table.header, "a");
    let b_idx = column_index(&table.header, "b");
    let count_idx = column_index(&table.header, "count");
    let mut out = BTreeMap::new();
    for row in table.rows {
        let a = row[a_idx].clone();
        let b = row[b_idx].clone();
        let count = parse_u64(&row[count_idx]);
        out.insert((a, b), count);
    }
    out
}

fn edges_for_weight(
    pairs: &BTreeMap<u64, BTreeMap<(String, String), u64>>,
    weight: u64,
) -> BTreeSet<(String, String)> {
    pairs
        .get(&weight)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

fn pair_count(
    pairs: &BTreeMap<u64, BTreeMap<(String, String), u64>>,
    weight: u64,
    a: &str,
    b: &str,
) -> u64 {
    pairs
        .get(&weight)
        .and_then(|map| map.get(&(a.to_string(), b.to_string())).copied())
        .unwrap_or(0)
}

fn aggregate_pairs(
    pairs: &BTreeMap<u64, BTreeMap<(String, String), u64>>,
) -> BTreeMap<(String, String), u64> {
    let mut out = BTreeMap::new();
    for map in pairs.values() {
        for (pair, count) in map {
            *out.entry(pair.clone()).or_insert(0) += count;
        }
    }
    out
}
