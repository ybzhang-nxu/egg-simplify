use std::path::PathBuf;

use mpl_experiments::{load_spec, parse_spec_str, run_count_only, ErrorCode, Status};

fn m5_spec_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("experiments")
        .join("m5")
        .join(name)
}

#[test]
fn m5_genealogical_count_only() {
    let cases = [
        ("M5_reg_gene_channel_no_interleave_w3.toml", 15_usize),
        ("M5_reg_gene_letter_a_forbid_b_w3.toml", 20_usize),
    ];

    for (spec_name, expected) in cases {
        let mut cfg = load_spec(&m5_spec_path(spec_name)).expect("load spec");
        cfg.out_dir = PathBuf::from("unused");
        let report = run_count_only(&cfg).expect("run count-only");
        let summary = report
            .summaries
            .iter()
            .find(|summary| summary.weight == 3)
            .expect("weight=3 summary");
        assert_eq!(summary.status, Status::Ok);
        assert!(summary.error_code.is_none());
        assert_eq!(summary.n_words_allowed, expected);
    }
}

#[test]
fn m5_genealogical_budget_exceeded() {
    let mut cfg =
        load_spec(&m5_spec_path("M5_reg_gene_budget_exhaust_w4.toml")).expect("load spec");
    cfg.out_dir = PathBuf::from("unused");
    let report = run_count_only(&cfg).expect("run count-only");
    let summary = report
        .summaries
        .iter()
        .find(|summary| summary.weight == 4)
        .expect("weight=4 summary");
    assert_eq!(summary.status, Status::Err);
    assert_eq!(
        summary.error_code,
        Some(ErrorCode::ConstraintBudgetExceeded)
    );
}

#[test]
fn genealogical_requires_channels_in_channel_mode() {
    let spec = r#"
[experiment]
id = "M5_missing_channel"
out_dir = "reports/m5/M5_missing_channel"
w_min = 3
w_max = 3

[alphabet]
vars = ["x"]

[[alphabet.letters]]
name = "a"
expr = "x"
channel = "0"

[[alphabet.letters]]
name = "b"
expr = "x"

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "genealogical"
seen = "channel"
rules = [{ if_seen = "0", forbid = ["1"] }]
"#;

    let err = parse_spec_str(spec).expect_err("expected invalid spec");
    let msg = err.to_string();
    assert!(msg.contains("InvalidSpecMissingChannel"));
}

#[test]
fn genealogical_named_channels_are_supported() {
    let spec = r#"
[experiment]
id = "M5_named_channel_regression"
out_dir = "reports/m5/M5_named_channel_regression"
w_min = 3
w_max = 3

[alphabet]
vars = ["x"]

[[alphabet.letters]]
name = "a"
expr = "x"
channel = "A"

[[alphabet.letters]]
name = "b"
expr = "x"
channel = "B"

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "genealogical"
seen = "channel"
rules = [{ if_seen = "A", forbid = ["B"] }]
"#;

    let cfg = parse_spec_str(spec).expect("parse spec");
    let report = run_count_only(&cfg).expect("run count-only");
    let summary = report
        .summaries
        .iter()
        .find(|summary| summary.weight == 3)
        .expect("weight=3 summary");
    assert_eq!(summary.status, Status::Ok);
    assert_eq!(summary.n_words_allowed, 4);
}
