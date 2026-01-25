use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use mpl_experiments::{ladder_de_down, ladder_symbol_bruteforce, ladder_symbol_combinatorial};
use mpl_symbol::space::check_integrable_n;

const LREF: usize = 5;

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
fn combinatorial_matches_bruteforce_small() {
    for loop_value in 1..=LREF {
        let comb = ladder_symbol_combinatorial(loop_value).expect("combinatorial symbol");
        let brute = ladder_symbol_bruteforce(loop_value).expect("bruteforce symbol");
        assert_eq!(comb, brute, "mismatch at L={loop_value}");
    }
}

#[test]
fn loop_lowering_de_matches_small() {
    let mut symbols = Vec::with_capacity(LREF);
    for loop_value in 1..=LREF {
        symbols.push(
            ladder_symbol_combinatorial(loop_value).expect("combinatorial symbol"),
        );
    }
    for loop_value in 2..=LREF {
        let sym = &symbols[loop_value - 1];
        let prev = &symbols[loop_value - 2];
        let (down1, down2) = ladder_de_down(sym);
        assert_eq!(down1, *prev, "zbar then z mismatch at L={loop_value}");
        assert_eq!(down2, *prev, "z then zbar mismatch at L={loop_value}");
    }
}

#[test]
fn integrability_small() {
    for loop_value in 1..=LREF {
        let sym = ladder_symbol_combinatorial(loop_value).expect("combinatorial symbol");
        let ok = check_integrable_n(&sym).expect("integrability check");
        assert!(ok, "integrability failed at L={loop_value}");
    }
}

#[test]
fn generator_matches_esymb_rank_scan_csvs() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let base_dir = std::env::temp_dir().join(format!(
        "mpl_ladder_contract_{}_{}",
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
        .arg("gen-ladder")
        .arg("--out-dir")
        .arg(&gen_dir)
        .arg("--data-dir")
        .arg(&jsonl_dir)
        .arg("--loops")
        .arg("2..5")
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
        .expect("run gen-ladder");
    assert!(status.success(), "gen-ladder failed");

    let status = Command::new(&exe)
        .arg("esymb-rank-scan")
        .arg("--data-dir")
        .arg(&jsonl_dir)
        .arg("--loops")
        .arg("2..5")
        .arg("--family")
        .arg("prefix-suffix")
        .arg("--prefix-len")
        .arg("2")
        .arg("--suffix-len")
        .arg("2")
        .arg("--letters")
        .arg("z")
        .arg("zbar")
        .arg("1-z")
        .arg("1-zbar")
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
