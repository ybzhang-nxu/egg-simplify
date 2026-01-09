use std::fs;
use std::path::{Path, PathBuf};

use mpl_experiments::{load_filtration_spec, run_filtration, write_filtration_summary};

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

fn value<'a>(table: &'a CsvTable, row: &'a [String], name: &str) -> &'a str {
    let idx = column_index(&table.header, name);
    &row[idx]
}

fn m6_spec_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("experiments")
        .join("m6")
        .join(name)
}

#[test]
fn m6_filtration_outputs_and_summary() {
    const HEADER: &str = "layer_index,layer_name,weight,mode,status,error_code,error,n_words_allowed,dim,rank,basis_ncols,rows_attempted,rows_inserted,samples_used,envs_total,constraints_insufficient_samples";
    let spec_path = m6_spec_path("M6_reg_filtration_chain_w3.toml");
    let mut spec = load_filtration_spec(&spec_path).expect("load filtration spec");
    let out_dir = prepare_dir("m6_filtration_chain");
    spec.out_dir = out_dir.clone();

    let report = run_filtration(&spec).expect("run filtration");
    write_filtration_summary(&report, &out_dir).expect("write summary");

    let summary_csv = out_dir.join("filtration_summary.csv");
    let summary_md = out_dir.join("filtration_summary.md");
    assert!(summary_csv.exists());
    assert!(summary_md.exists());

    let table = read_csv_table(&summary_csv);
    let summary_content = fs::read_to_string(&summary_csv).expect("read summary csv");
    let header_line = summary_content.lines().next().expect("summary header");
    assert_eq!(header_line, HEADER);

    let expected = [
        ("L0_integrability", 64_u64),
        ("L1_first_entry", 32),
        ("L2_steinmann_forbid_11", 24),
        ("L3_cluster_allow_00_01", 16),
        ("L4_gene_0_forbid_1", 8),
    ];

    for (layer, expected_count) in expected {
        let row = table
            .rows
            .iter()
            .find(|row| value(&table, row, "layer_name") == layer)
            .expect("missing layer row");
        assert_eq!(value(&table, row, "weight"), "3");
        assert_eq!(value(&table, row, "mode"), "count_only");
        assert_eq!(value(&table, row, "status"), "ok");
        assert!(value(&table, row, "error_code").is_empty());
        let count: u64 = value(&table, row, "n_words_allowed")
            .parse()
            .expect("parse n_words_allowed");
        assert_eq!(count, expected_count);
    }

    let layer_dir = out_dir.join("layers").join("0_L0_integrability").join("w3");
    assert!(layer_dir.join("counts_only.csv").exists());
}
