use std::fs;
use std::path::{Path, PathBuf};

use mpl_experiments::{load_spec, run_experiment, write_outputs};

struct Expected {
    n_vertices: u64,
    n_edges_undirected: u64,
    triangles: u64,
    clustering_num: u64,
    clustering_den: u64,
    beta1_est: i64,
    triplets_supported_by_triangles_num: u64,
    triplets_supported_by_triangles_den: u64,
}

struct CsvTable {
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn prepare_dir(name: &str) -> PathBuf {
    let path = PathBuf::from("target/tmp").join(name);
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create tmp dir");
    path
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

fn parse_i64(value: &str) -> i64 {
    value.parse().expect("parse i64")
}

fn m3_spec_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("experiments")
        .join("m3")
        .join(name)
}

#[test]
fn m3_skeleton2_metrics_regressions() {
    const HEADER: &str = "weight,status,error_code,error,n_vertices,n_edges_undirected,triangles,clustering_num,clustering_den,beta1_est,triplets_supported_by_triangles_num,triplets_supported_by_triangles_den";
    let cases = [
        (
            "M3_reg_k3_w3.toml",
            Expected {
                n_vertices: 3,
                n_edges_undirected: 3,
                triangles: 1,
                clustering_num: 3,
                clustering_den: 3,
                beta1_est: 1,
                triplets_supported_by_triangles_num: 3,
                triplets_supported_by_triangles_den: 3,
            },
        ),
        (
            "M3_reg_cycle12_w3.toml",
            Expected {
                n_vertices: 12,
                n_edges_undirected: 12,
                triangles: 0,
                clustering_num: 0,
                clustering_den: 12,
                beta1_est: 1,
                triplets_supported_by_triangles_num: 0,
                triplets_supported_by_triangles_den: 12,
            },
        ),
        (
            "M3_reg_chain_w3.toml",
            Expected {
                n_vertices: 3,
                n_edges_undirected: 2,
                triangles: 0,
                clustering_num: 0,
                clustering_den: 1,
                beta1_est: 0,
                triplets_supported_by_triangles_num: 0,
                triplets_supported_by_triangles_den: 1,
            },
        ),
    ];

    for (spec_name, expected) in cases {
        let spec_path = m3_spec_path(spec_name);
        let mut cfg = load_spec(&spec_path).expect("load spec");
        let dir_name = format!("m3_skeleton2_{}", cfg.name);
        let out_dir = prepare_dir(&dir_name);
        cfg.out_dir = out_dir.clone();

        let report = run_experiment(&cfg).expect("run experiment");
        write_outputs(&report, &out_dir).expect("write outputs");

        let csv_path = out_dir.join("skeleton2_metrics.csv");
        let content = fs::read_to_string(&csv_path).expect("read skeleton2_metrics.csv");
        let header_line = content.lines().next().expect("header line");
        assert_eq!(header_line, HEADER);

        let table = read_csv_table(&csv_path);
        let row = row_by_weight(&table, 3);
        assert_eq!(value(&table, row, "status"), "ok");
        assert!(value(&table, row, "error_code").is_empty());
        assert!(value(&table, row, "error").is_empty());

        assert_eq!(
            parse_u64(value(&table, row, "n_vertices")),
            expected.n_vertices
        );
        assert_eq!(
            parse_u64(value(&table, row, "n_edges_undirected")),
            expected.n_edges_undirected
        );
        assert_eq!(
            parse_u64(value(&table, row, "triangles")),
            expected.triangles
        );
        assert_eq!(
            parse_u64(value(&table, row, "clustering_num")),
            expected.clustering_num
        );
        assert_eq!(
            parse_u64(value(&table, row, "clustering_den")),
            expected.clustering_den
        );
        assert_eq!(
            parse_i64(value(&table, row, "beta1_est")),
            expected.beta1_est
        );
        assert_eq!(
            parse_u64(value(&table, row, "triplets_supported_by_triangles_num")),
            expected.triplets_supported_by_triangles_num
        );
        assert_eq!(
            parse_u64(value(&table, row, "triplets_supported_by_triangles_den")),
            expected.triplets_supported_by_triangles_den
        );
    }
}
