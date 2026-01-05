use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn bin_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_mpl_simplify") {
        return PathBuf::from(path);
    }
    let exe = std::env::current_exe().expect("current exe");
    let target_dir = exe
        .parent()
        .and_then(|dir| dir.parent())
        .expect("target dir");
    let mut bin = target_dir.join("mpl-simplify");
    if cfg!(windows) {
        bin.set_extension("exe");
    }
    bin
}

#[test]
fn cli_normalize_basic() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args(["normalize", "--expr", "(+ x y 0 3 x)"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("(+ 3 x x y)"));
}

#[test]
fn cli_rational_literal() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args(["normalize", "--expr", "1/2"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("1/2"));
}

#[test]
fn cli_div_mul_simplifies() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args(["normalize", "--expr", "(* 2 (/ 1 2))"]);
    cmd.assert().success().stdout(predicate::str::contains("1"));
}

#[test]
fn cli_pow_nesting_simplifies() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args(["normalize", "--expr", "(^ (^ x 2) 3)"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("(^ x 6)"));
}

#[test]
fn cli_symbol_li2_contains_tensor() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args(["symbol", "--expr", "(li2 x)"]);
    cmd.assert().success().stdout(predicate::str::contains("⊗"));
}

#[test]
fn cli_check_integrable_log_log() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args(["check-integrable", "--expr", "(* (log x) (log y))"]);
    cmd.assert().success().stdout(predicate::eq("true\n"));
}

#[test]
fn cli_simplify_aggressive_factors() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args(["simplify", "--aggressive", "--expr", "(+ (* x y) (* x z))"]);
    let pred = predicate::str::contains("(+ y z)").and(predicate::str::contains("x"));
    cmd.assert().success().stdout(pred);
}

#[test]
fn cli_simplify_guard_blocks_li2_factoring() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());
    let mut cmd = Command::new(exe);
    cmd.args([
        "simplify",
        "--aggressive",
        "--expr",
        "(li2 (+ (* x y) (* x z)))",
    ]);
    let pred = predicate::str::contains("(li2")
        .and(predicate::str::contains("(* x y)"))
        .and(predicate::str::contains("(* x z)"))
        .and(predicate::str::contains("(+ y z)").not());
    cmd.assert().success().stdout(pred);
}
