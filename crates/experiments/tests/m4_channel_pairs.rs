use std::path::PathBuf;

use mpl_experiments::{load_spec, parse_spec_str, run_count_only, Status};

fn m4_spec_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("experiments")
        .join("m4")
        .join(name)
}

#[test]
fn m4_channel_pairs_count_only() {
    let cases = [
        ("M4_reg_steinmann_no_repeat_channel_w3.toml", 16_usize),
        ("M4_reg_cluster_same_channel_w3.toml", 28_usize),
    ];

    for (spec_name, expected) in cases {
        let mut cfg = load_spec(&m4_spec_path(spec_name)).expect("load spec");
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
fn channel_pairs_requires_channels() {
    let spec = r#"
[experiment]
id = "M4_missing_channel"
out_dir = "reports/m4/M4_missing_channel"
w_min = 3
w_max = 3

[alphabet]
vars = ["x"]

[[alphabet.letters]]
name = "a"
expr = "x"
channel = 0

[[alphabet.letters]]
name = "b"
expr = "x"

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "channel_pairs"
mode = "allowed"
pairs = [[0, 0]]
"#;

    let err = parse_spec_str(spec).expect_err("expected invalid spec");
    let msg = err.to_string();
    assert!(msg.contains("InvalidSpecMissingChannel"));
}

#[test]
fn channel_numeric_strings_are_canonicalized() {
    let spec = r#"
[experiment]
id = "M4_channel_numeric_normalization"
out_dir = "unused"
w_min = 2
w_max = 2

[alphabet]
vars = ["x"]

[[alphabet.letters]]
name = "a"
expr = "x"
channel = "01"

[[alphabet.letters]]
name = "b"
expr = "x"
channel = 1

[[alphabet.letters]]
name = "c"
expr = "x"
channel = 2

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "genealogical"
seen = "channel"
rules = [{ if_seen = "1", forbid = ["1"] }]

[[constraints.automaton.acceptors]]
kind = "channel_pairs"
mode = "forbidden"
symmetric = true
pairs = []
"#;

    let cfg = parse_spec_str(spec).expect("parse spec");
    let report = run_count_only(&cfg).expect("count-only");
    let summary = report
        .summaries
        .iter()
        .find(|summary| summary.weight == 2)
        .expect("weight=2 summary");
    assert_eq!(summary.status, Status::Ok);
    assert_eq!(summary.n_words_allowed, 5);
}

#[test]
fn channel_numeric_strings_match_genealogical_and_pairs() {
    let spec = r#"
[experiment]
id = "M4_channel_numeric_crosscheck"
out_dir = "unused"
w_min = 2
w_max = 2

[alphabet]
vars = ["x"]

[[alphabet.letters]]
name = "a"
expr = "x"
channel = "01"

[[alphabet.letters]]
name = "b"
expr = "x"
channel = 1

[[alphabet.letters]]
name = "c"
expr = "x"
channel = 2

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "genealogical"
seen = "channel"
rules = [{ if_seen = "1", forbid = ["2"] }]

[[constraints.automaton.acceptors]]
kind = "channel_pairs"
mode = "allowed"
pairs = [[1, 2]]
"#;

    let cfg = parse_spec_str(spec).expect("parse spec");
    let report = run_count_only(&cfg).expect("count-only");
    let summary = report
        .summaries
        .iter()
        .find(|summary| summary.weight == 2)
        .expect("weight=2 summary");
    assert_eq!(summary.status, Status::Ok);
    assert_eq!(summary.n_words_allowed, 0);
}

#[test]
fn channel_pairs_support_named_channels() {
    let spec = r#"
[experiment]
id = "M4_named_channel_pairs"
out_dir = "unused"
w_min = 2
w_max = 2

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
kind = "channel_pairs"
mode = "forbidden"
symmetric = true
pairs = [["A", "B"]]
"#;

    let cfg = parse_spec_str(spec).expect("parse spec");
    let report = run_count_only(&cfg).expect("count-only");
    let summary = report
        .summaries
        .iter()
        .find(|summary| summary.weight == 2)
        .expect("weight=2 summary");
    assert_eq!(summary.status, Status::Ok);
    assert_eq!(summary.n_words_allowed, 2);
}
