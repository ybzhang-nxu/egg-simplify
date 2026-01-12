use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use mpl_experiments::{
    parse_spec_str, render_basis_stats, render_dim_vs_w, render_pairs, render_pairs_by_weight,
    render_skeleton2_metrics, render_topology_metrics, render_triplets, render_triplets_by_weight,
    run_experiment, toy_alphabet_xy, write_outputs, ErrorCode, ExperimentConfig, Status,
};
use mpl_symbol::space::{ConstraintBudget, SampleTable, WordConstraints};

fn count_words(alpha_len: usize, constraints: &WordConstraints, weight: usize) -> usize {
    fn rec(
        alpha_len: usize,
        constraints: &WordConstraints,
        weight: usize,
        pos: usize,
        prev: Option<usize>,
    ) -> usize {
        if pos == weight {
            return 1;
        }
        let mut total = 0;
        for next in 0..alpha_len {
            if constraints.allow_step(pos, prev, next) {
                total += rec(alpha_len, constraints, weight, pos + 1, Some(next));
            }
        }
        total
    }
    if weight == 0 {
        return 1;
    }
    rec(alpha_len, constraints, weight, 0, None)
}

fn prepare_dir(name: &str) -> PathBuf {
    let path = PathBuf::from("target/tmp").join(name);
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create tmp dir");
    path
}

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct EnvVarGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let prev = std::env::var(key).ok();
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

fn collect_files(root: &std::path::Path) -> Vec<PathBuf> {
    fn rec(root: &std::path::Path, dir: &std::path::Path, files: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .expect("read dir")
            .map(|entry| entry.expect("read dir entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                rec(root, &path, files);
            } else {
                let rel = path.strip_prefix(root).expect("strip prefix").to_path_buf();
                files.push(rel);
            }
        }
    }

    let mut files = Vec::new();
    rec(root, root, &mut files);
    files.sort();
    files
}

fn assert_dirs_equal(left: &std::path::Path, right: &std::path::Path) {
    let left_files = collect_files(left);
    let right_files = collect_files(right);
    assert_eq!(left_files, right_files, "file lists differ");
    for rel in left_files {
        let left_bytes = fs::read(left.join(&rel)).expect("read left file");
        let right_bytes = fs::read(right.join(&rel)).expect("read right file");
        assert_eq!(left_bytes, right_bytes, "content mismatch for {rel:?}");
    }
}

#[test]
fn toy_xy_dim_is_w_plus_1() {
    let cfg = ExperimentConfig {
        name: "toy_xy".to_string(),
        out_dir: PathBuf::from("unused"),
        alphabet: toy_alphabet_xy(),
        constraints: WordConstraints::default(),
        genealogical_acceptors: Vec::new(),
        kgram_acceptors: Vec::new(),
        channel_pairs_acceptors: Vec::new(),
        automaton_acceptors: Vec::new(),
        constraint_budget: ConstraintBudget::default(),
        weight_min: 1,
        weight_max: 6,
        vars: Vec::new(),
        sample_table: SampleTable::default(),
    };

    let report = run_experiment(&cfg).unwrap();
    for summary in &report.summaries {
        assert_eq!(summary.stats.dim, summary.weight + 1);
    }
}

#[test]
fn adjacency_constraint_removes_pair_and_matches_word_count() {
    let mut allowed_pairs = vec![vec![true; 2]; 2];
    allowed_pairs[1][0] = false;
    let constraints = WordConstraints {
        first_allowed: None,
        allowed_pairs: Some(allowed_pairs),
    };

    let cfg = ExperimentConfig {
        name: "toy_xy_no_y_to_x".to_string(),
        out_dir: PathBuf::from("unused"),
        alphabet: toy_alphabet_xy(),
        constraints: constraints.clone(),
        genealogical_acceptors: Vec::new(),
        kgram_acceptors: Vec::new(),
        channel_pairs_acceptors: Vec::new(),
        automaton_acceptors: Vec::new(),
        constraint_budget: ConstraintBudget::default(),
        weight_min: 3,
        weight_max: 3,
        vars: Vec::new(),
        sample_table: SampleTable::default(),
    };

    let report = run_experiment(&cfg).unwrap();
    let pairs_csv = render_pairs(&report);
    for line in pairs_csv.lines().skip(1) {
        let parts: Vec<&str> = line.split(',').collect();
        assert!(parts.len() >= 3);
        if parts[0] == "y" && parts[1] == "x" {
            panic!("found forbidden pair in pairs.csv");
        }
    }
    let pairs_by_weight_csv = render_pairs_by_weight(&report);
    for line in pairs_by_weight_csv.lines().skip(1) {
        let parts: Vec<&str> = line.split(',').collect();
        assert!(parts.len() >= 4);
        if parts[1] == "y" && parts[2] == "x" {
            panic!("found forbidden pair in pairs_by_weight.csv");
        }
    }

    let expected = count_words(2, &constraints, 3);
    assert_eq!(report.summaries[0].n_words_allowed, expected);
}

#[test]
fn outputs_are_deterministic() {
    let cfg = ExperimentConfig {
        name: "toy_xy_determinism".to_string(),
        out_dir: PathBuf::from("unused"),
        alphabet: toy_alphabet_xy(),
        constraints: WordConstraints::default(),
        genealogical_acceptors: Vec::new(),
        kgram_acceptors: Vec::new(),
        channel_pairs_acceptors: Vec::new(),
        automaton_acceptors: Vec::new(),
        constraint_budget: ConstraintBudget::default(),
        weight_min: 2,
        weight_max: 4,
        vars: Vec::new(),
        sample_table: SampleTable::default(),
    };

    let r1 = run_experiment(&cfg).unwrap();
    let r2 = run_experiment(&cfg).unwrap();

    assert_eq!(render_basis_stats(&r1), render_basis_stats(&r2));
    assert_eq!(render_dim_vs_w(&r1), render_dim_vs_w(&r2));
    assert_eq!(render_pairs(&r1), render_pairs(&r2));
    assert_eq!(render_pairs_by_weight(&r1), render_pairs_by_weight(&r2));
    assert_eq!(render_triplets(&r1), render_triplets(&r2));
    assert_eq!(
        render_triplets_by_weight(&r1),
        render_triplets_by_weight(&r2)
    );
    assert_eq!(render_topology_metrics(&r1), render_topology_metrics(&r2));
    assert_eq!(render_skeleton2_metrics(&r1), render_skeleton2_metrics(&r2));
}

#[test]
fn outputs_are_deterministic_across_space_threads() {
    let spec = r#"
[experiment]
id = "T_parallel_space_threads"
out_dir = "reports/m1/T_parallel_space_threads"
w_min = 2
w_max = 3

[alphabet]
vars = ["x", "y"]

[[alphabet.letters]]
name = "a"
expr = "(+ 1 x)"

[[alphabet.letters]]
name = "b"
expr = "(+ 2 y)"

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []
"#;

    let _lock = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _chunk_guard = EnvVarGuard::set("MPL_SPACE_CONTEXT_CHUNK", Some("4"));

    let cfg = parse_spec_str(spec).unwrap();
    let out_dir_serial = prepare_dir("space_threads_1");
    let out_dir_parallel = prepare_dir("space_threads_8");

    {
        let _threads_guard = EnvVarGuard::set("MPL_SPACE_THREADS", Some("1"));
        let mut cfg = cfg.clone();
        cfg.out_dir = out_dir_serial.clone();
        let report = run_experiment(&cfg).unwrap();
        write_outputs(&report, &cfg.out_dir).unwrap();
    }

    {
        let _threads_guard = EnvVarGuard::set("MPL_SPACE_THREADS", Some("8"));
        let mut cfg = cfg.clone();
        cfg.out_dir = out_dir_parallel.clone();
        let report = run_experiment(&cfg).unwrap();
        write_outputs(&report, &cfg.out_dir).unwrap();
    }

    assert_dirs_equal(&out_dir_serial, &out_dir_parallel);
}

#[test]
fn parse_spec_builds_constraints() {
    let spec = r#"
[experiment]
id = "T0"
title = "test"
out_dir = "reports/m1/T0"
w_min = 1
w_max = 2

[alphabet]
vars = ["x", "y"]

[[alphabet.letters]]
name = "x"
expr = "x"

[[alphabet.letters]]
name = "y"
expr = "y"

[constraints]
first_entry = ["x"]
adjacency_mode = "forbid"
adjacency_pairs = [["y","x"]]

[pairs]
count_mode = "active_word_positions"
"#;

    let cfg = parse_spec_str(spec).unwrap();
    assert_eq!(cfg.alphabet.letters.len(), 2);
    assert_eq!(cfg.alphabet.letter_names, vec!["x", "y"]);
    let pairs = cfg.constraints.allowed_pairs.unwrap();
    assert!(!pairs[1][0]);
    assert!(pairs[0][1]);
    let first = cfg.constraints.first_allowed.unwrap();
    assert!(first.contains(&0));
    assert!(!first.contains(&1));
}

#[test]
fn parse_spec_builds_kgram_acceptor() {
    let spec = kgram_spec();
    let cfg = parse_spec_str(spec).unwrap();
    assert_eq!(cfg.kgram_acceptors.len(), 1);
    assert_eq!(cfg.kgram_acceptors[0].triplets().len(), 2);
}

#[test]
fn kgram_allow_requires_triplets() {
    let spec = kgram_spec_empty_allow();
    let err = parse_spec_str(spec).expect_err("expected error");
    let msg = err.to_string();
    assert!(msg.contains("InvalidSpecEmptyAllowList"));
}

#[test]
fn triplet_outputs_from_kgram_spec() {
    let spec = kgram_spec();
    let mut cfg = parse_spec_str(spec).unwrap();
    let out_dir = prepare_dir("kgram_triplets");
    cfg.out_dir = out_dir.clone();

    let report = run_experiment(&cfg).unwrap();
    write_outputs(&report, &out_dir).unwrap();

    let triplets = fs::read_to_string(out_dir.join("triplets.csv")).unwrap();
    let triplets_by_weight = fs::read_to_string(out_dir.join("triplets_by_weight.csv")).unwrap();

    let expected_triplets = "a,b,c,count\na,b,c,1\nb,c,a,1\n";
    let expected_triplets_by_weight = "weight,a,b,c,count\n3,a,b,c,1\n3,b,c,a,1\n";

    assert_eq!(triplets, expected_triplets);
    assert_eq!(triplets_by_weight, expected_triplets_by_weight);
}

#[test]
fn genealogical_budget_exceeded_maps_error_code() {
    let spec = genealogical_budget_spec();
    let cfg = parse_spec_str(spec).unwrap();
    let report = run_experiment(&cfg).unwrap();

    assert_eq!(report.summaries.len(), 2);
    let summary_w1 = &report.summaries[0];
    let summary_w2 = &report.summaries[1];
    assert_eq!(summary_w1.status, Status::Ok);
    assert_eq!(summary_w1.error_code, None);
    assert_eq!(summary_w2.status, Status::Err);
    assert_eq!(
        summary_w2.error_code,
        Some(ErrorCode::ConstraintBudgetExceeded)
    );
}

fn kgram_spec() -> &'static str {
    r#"
[experiment]
id = "K0"
out_dir = "unused"
w_min = 3
w_max = 3

[alphabet]
vars = ["x"]

[[alphabet.letters]]
name = "a"
expr = "x"

[[alphabet.letters]]
name = "b"
expr = "x"

[[alphabet.letters]]
name = "c"
expr = "x"

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "kgram"
k = 3
mode = "allowed"
triplets = [["a","b","c"], ["b","c","a"]]
"#
}

fn kgram_spec_empty_allow() -> &'static str {
    r#"
[experiment]
id = "K0_empty"
out_dir = "unused"
w_min = 3
w_max = 3

[alphabet]
vars = ["x"]

[[alphabet.letters]]
name = "a"
expr = "x"

[[alphabet.letters]]
name = "b"
expr = "x"

[[alphabet.letters]]
name = "c"
expr = "x"

[constraints]
adjacency_mode = "forbid"
adjacency_pairs = []

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "kgram"
k = 3
mode = "allowed"
triplets = []
"#
}

fn genealogical_budget_spec() -> &'static str {
    r#"
[experiment]
id = "G0"
out_dir = "unused"
w_min = 1
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

[constraints.budget]
max_states = 3

[constraints.automaton]
[[constraints.automaton.acceptors]]
kind = "genealogical"
seen = "channel"
rules = [
  { if_seen = "A", forbid = ["A"] },
  { if_seen = "B", forbid = ["B"] }
]
"#
}
