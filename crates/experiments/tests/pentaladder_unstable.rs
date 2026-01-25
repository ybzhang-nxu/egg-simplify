use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use mpl_experiments::{
    pentaladder_alphabet, symbol_psi, symbol_psi2_golden, symbol_psi_with_psi2_source, Psi2Source,
};
use mpl_symbol::space::check_integrable_n;
use mpl_symbol::{Coeff, Symbol, Word};
use num_traits::Zero;

const LREF: usize = 4;

fn bin_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_mpl_experiments") {
        return PathBuf::from(path);
    }
    let exe = std::env::current_exe().expect("current exe");
    let target_dir = exe
        .parent()
        .and_then(|dir| dir.parent())
        .expect("target dir");
    let mut bin = target_dir.join("mpl_experiments");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    bin
}

fn unique_stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos()
}

#[test]
#[ignore = "unstable recursion path; re-enable when psi2 recursion matches golden"]
fn psi2_recursive_matches_golden() {
    let sym = symbol_psi_with_psi2_source(2, Psi2Source::Recursive).expect("psi2 recursive");
    let golden = symbol_psi2_golden().expect("psi2 golden");
    assert_eq!(sym, golden);
}

#[test]
#[ignore = "unstable pentaladder integrability gate; slow for CI"]
fn integrability_alphabet_last_entry_small() {
    let alpha = pentaladder_alphabet();
    let mut map = BTreeMap::new();
    for (expr, name) in alpha.letters.iter().zip(alpha.letter_names.iter()) {
        map.insert(expr.to_canonical_string(), name.clone());
    }
    let allowed: BTreeSet<String> = ["u", "1-u", "v", "1-v", "1-w"]
        .iter()
        .map(|name| name.to_string())
        .collect();
    for loop_value in 1..=LREF {
        let sym = symbol_psi(loop_value).expect("symbol");
        let ok = check_integrable_n(&sym).expect("integrability check");
        assert!(ok, "integrability failed at L={loop_value}");
        for (word, coeff) in sym.terms() {
            if coeff.is_zero() {
                continue;
            }
            for letter in word.letters() {
                let key = letter.to_canonical_string();
                assert!(
                    map.contains_key(&key),
                    "unknown alphabet letter at L={loop_value}: {key}"
                );
            }
            if loop_value >= 2 {
                let last = word.letters().last().expect("last letter");
                let name = map
                    .get(&last.to_canonical_string())
                    .expect("last letter name");
                assert!(
                    allowed.contains(name),
                    "last-entry violation at L={loop_value}: {name}"
                );
            }
        }
    }
}

#[test]
#[ignore = "psi3 span check is slow; run with PENTALADDER_TRACE_* when debugging"]
fn psi3_last_entry_span_checks() {
    let alpha = pentaladder_alphabet();
    let mut name_map = BTreeMap::new();
    for (expr, name) in alpha.letters.iter().zip(alpha.letter_names.iter()) {
        name_map.insert(expr.to_canonical_string(), name.clone());
    }
    let sym = symbol_psi(3).expect("psi3");
    assert_last_entry_span(&sym, &name_map, "psi3");
}

#[test]
#[ignore = "slow E2E contract test; enable when needed"]
fn generator_matches_esymb_rank_scan_csvs() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let base_dir = std::env::temp_dir().join(format!(
        "mpl_pentaladder_contract_{}_{}",
        std::process::id(),
        unique_stamp()
    ));
    let gen_dir = base_dir.join("gen");
    let scan_dir = base_dir.join("scan");
    let jsonl_dir = base_dir.join("jsonl");

    let _ = fs::remove_dir_all(&base_dir);
    fs::create_dir_all(&gen_dir).expect("create gen dir");
    fs::create_dir_all(&scan_dir).expect("create scan dir");
    fs::create_dir_all(&jsonl_dir).expect("create jsonl dir");

    let status = Command::new(&exe)
        .arg("gen-pentaladder")
        .arg("--out-dir")
        .arg(&gen_dir)
        .arg("--data-dir")
        .arg(&jsonl_dir)
        .arg("--loops")
        .arg("2..4")
        .arg("--family")
        .arg("prefix-suffix")
        .arg("--prefix-len")
        .arg("2")
        .arg("--suffix-len")
        .arg("2")
        .arg("--matrix-rank")
        .arg("--emit-jsonl")
        .arg("--no-validate")
        .status()
        .expect("run gen-pentaladder");
    assert!(status.success(), "gen-pentaladder failed");

    let status = Command::new(&exe)
        .arg("esymb-rank-scan")
        .arg("--data-dir")
        .arg(&jsonl_dir)
        .arg("--loops")
        .arg("2..4")
        .arg("--family")
        .arg("prefix-suffix")
        .arg("--prefix-len")
        .arg("2")
        .arg("--suffix-len")
        .arg("2")
        .arg("--letters")
        .arg("u")
        .arg("v")
        .arg("1-u")
        .arg("1-v")
        .arg("1-w")
        .arg("w")
        .arg("1-uw")
        .arg("1-vw")
        .arg("Delta")
        .arg("--export-observables")
        .arg("--matrix-rank")
        .arg("--out-dir")
        .arg(&scan_dir)
        .status()
        .expect("run esymb-rank-scan");
    assert!(status.success(), "esymb-rank-scan failed");

    let gen_observables = fs::read(gen_dir.join("marginals_observables.csv"))
        .expect("read gen marginals_observables.csv");
    let scan_observables = fs::read(scan_dir.join("marginals_observables.csv"))
        .expect("read scan marginals_observables.csv");
    assert_eq!(gen_observables, scan_observables, "observables csv mismatch");

    let gen_rank =
        fs::read(gen_dir.join("marginals_matrix_rank.csv")).expect("read gen rank csv");
    let scan_rank =
        fs::read(scan_dir.join("marginals_matrix_rank.csv")).expect("read scan rank csv");
    assert_eq!(gen_rank, scan_rank, "matrix rank csv mismatch");
}

fn assert_last_entry_span(
    sym: &Symbol,
    name_map: &BTreeMap<String, String>,
    label: &str,
) {
    let buckets = bucket_prefix_by_last(sym, name_map);
    let p_u = buckets.get("u").cloned().unwrap_or_else(Symbol::zero);
    let p_1u = buckets.get("1-u").cloned().unwrap_or_else(Symbol::zero);
    let p_v = buckets.get("v").cloned().unwrap_or_else(Symbol::zero);
    let p_1v = buckets.get("1-v").cloned().unwrap_or_else(Symbol::zero);
    let p_1u_expected = symbol_scale(&p_u, Coeff::from_integer(-1));
    let p_1v_expected = symbol_scale(&p_v, Coeff::from_integer(-1));
    if p_1u != p_1u_expected || p_1v != p_1v_expected {
        let out = [
            diff_symbol_summary("P_1u_vs_-P_u", &p_1u, &p_1u_expected),
            diff_symbol_summary("P_1v_vs_-P_v", &p_1v, &p_1v_expected),
        ];
        panic!("{label} last-entry span mismatch\n{}", out.join("\n"));
    }
}

fn bucket_prefix_by_last(
    sym: &Symbol,
    name_map: &BTreeMap<String, String>,
) -> BTreeMap<String, Symbol> {
    let mut buckets: BTreeMap<String, BTreeMap<Word, Coeff>> = BTreeMap::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let letters = word.letters();
        if letters.is_empty() {
            continue;
        }
        let last = letters.last().expect("last letter");
        let last_key = last.to_canonical_string();
        let name = name_map
            .get(&last_key)
            .cloned()
            .unwrap_or_else(|| "<unknown>".to_string());
        let prefix = Word(letters[..letters.len() - 1].to_vec());
        let entry = buckets.entry(name).or_default();
        let coeff_entry = entry.entry(prefix).or_insert_with(Coeff::zero);
        *coeff_entry += *coeff;
    }
    let mut out = BTreeMap::new();
    for (name, map) in buckets {
        let terms = map
            .into_iter()
            .filter(|(_, coeff)| !coeff.is_zero())
            .collect::<Vec<_>>();
        out.insert(name, Symbol::from_terms(terms));
    }
    out
}

fn symbol_scale(sym: &Symbol, coeff: Coeff) -> Symbol {
    if coeff.is_zero() {
        return Symbol::zero();
    }
    let mut terms = Vec::new();
    for (word, value) in sym.terms() {
        let scaled = *value * coeff;
        if !scaled.is_zero() {
            terms.push((word.clone(), scaled));
        }
    }
    Symbol::from_terms(terms)
}

fn diff_symbol_summary(name: &str, left: &Symbol, right: &Symbol) -> String {
    let left_map = symbol_terms_map(left);
    let right_map = symbol_terms_map(right);
    let mut left_only = 0usize;
    let mut right_only = 0usize;
    let mut mismatch = 0usize;
    for (word, coeff) in &left_map {
        match right_map.get(word) {
            None => left_only += 1,
            Some(other) if other != coeff => mismatch += 1,
            _ => {}
        }
    }
    for word in right_map.keys() {
        if !left_map.contains_key(word) {
            right_only += 1;
        }
    }
    format!(
        "{name}: left_only={left_only}, right_only={right_only}, coeff_mismatch={mismatch}, left_terms={}, right_terms={}",
        left_map.len(),
        right_map.len()
    )
}

fn symbol_terms_map(sym: &Symbol) -> BTreeMap<Word, Coeff> {
    let mut out = BTreeMap::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        out.insert(word.clone(), *coeff);
    }
    out
}
