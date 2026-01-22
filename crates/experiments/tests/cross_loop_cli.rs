use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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

#[test]
fn cross_loop_cli_writes_report() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let out_dir = std::env::temp_dir().join(format!("mpl_cross_loop_cli_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let status = Command::new(exe)
        .arg("cross-loop")
        .arg("--spec")
        .arg(spec_path())
        .arg("--weight")
        .arg("2")
        .arg("--suffix")
        .arg("x")
        .arg("x")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments cross-loop");
    assert!(status.success());

    let report_path = out_dir.join("cross_loop_report.txt");
    assert!(report_path.exists());
    let contents = fs::read_to_string(report_path).expect("read report");
    assert!(contents.contains("image_rank="));

    let matrix_path = out_dir.join("mapping_matrix.csv");
    assert!(matrix_path.exists());
}

#[test]
fn cross_loop_cli_scan_multi_suffix_writes_index() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let out_dir =
        std::env::temp_dir().join(format!("mpl_cross_loop_scan_multi_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let status = Command::new(exe)
        .arg("cross-loop")
        .arg("--spec")
        .arg(spec_path())
        .arg("--weight-min")
        .arg("2")
        .arg("--weight-max")
        .arg("2")
        .arg("--suffix")
        .arg("x")
        .arg("x")
        .arg("--suffix")
        .arg("y")
        .arg("y")
        .arg("--out")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments cross-loop scan");
    assert!(status.success());

    let index_path = out_dir.join("cross_loop_scan_index.csv");
    assert!(index_path.exists());

    let x_dir = out_dir.join("x_x");
    let y_dir = out_dir.join("y_y");
    assert!(x_dir.join("cross_loop_scan.csv").exists());
    assert!(y_dir.join("cross_loop_scan.csv").exists());
}

#[test]
fn cross_loop_cli_scan_suffixes_toml_writes_index() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let out_dir =
        std::env::temp_dir().join(format!("mpl_cross_loop_scan_toml_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let suffixes_path = std::env::temp_dir().join(format!(
        "mpl_cross_loop_suffixes_{}_{}.toml",
        std::process::id(),
        unique_stamp()
    ));
    fs::write(
        &suffixes_path,
        "suffixes = [[\"x\", \"x\"], [\"y\", \"y\"]]\n",
    )
    .expect("write suffixes toml");

    let status = Command::new(exe)
        .arg("cross-loop")
        .arg("--spec")
        .arg(spec_path())
        .arg("--weight-min")
        .arg("2")
        .arg("--weight-max")
        .arg("2")
        .arg("--suffixes-toml")
        .arg(&suffixes_path)
        .arg("--out")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments cross-loop scan");
    assert!(status.success());

    let index_path = out_dir.join("cross_loop_scan_index.csv");
    assert!(index_path.exists());

    let x_dir = out_dir.join("x_x");
    let y_dir = out_dir.join("y_y");
    assert!(x_dir.join("cross_loop_scan.csv").exists());
    assert!(y_dir.join("cross_loop_scan.csv").exists());
}

#[test]
fn cross_loop_cli_scan_suffixes_toml_dedups_duplicates() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let out_dir =
        std::env::temp_dir().join(format!("mpl_cross_loop_scan_dedup_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let suffixes_path = std::env::temp_dir().join(format!(
        "mpl_cross_loop_suffixes_dedup_{}_{}.toml",
        std::process::id(),
        unique_stamp()
    ));
    fs::write(
        &suffixes_path,
        "suffixes = [[\"x\", \"x\"], [\"x\", \"x\"], [\"y\", \"y\"]]\n",
    )
    .expect("write suffixes toml");

    let status = Command::new(exe)
        .arg("cross-loop")
        .arg("--spec")
        .arg(spec_path())
        .arg("--weight-min")
        .arg("2")
        .arg("--weight-max")
        .arg("2")
        .arg("--suffixes-toml")
        .arg(&suffixes_path)
        .arg("--out")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments cross-loop scan");
    assert!(status.success());

    let index_path = out_dir.join("cross_loop_scan_index.csv");
    let contents = fs::read_to_string(index_path).expect("read index");
    let rows = contents
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .count();
    assert_eq!(rows, 2);
    assert!(out_dir.join("x_x").exists());
    assert!(out_dir.join("y_y").exists());
    assert!(!out_dir.join("x_x_1").exists());
}

#[test]
fn cross_loop_cli_scan_suffixes_toml_unknown_letter_errors() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let out_dir = std::env::temp_dir().join(format!(
        "mpl_cross_loop_scan_unknown_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let suffixes_path = std::env::temp_dir().join(format!(
        "mpl_cross_loop_suffixes_unknown_{}_{}.toml",
        std::process::id(),
        unique_stamp()
    ));
    fs::write(
        &suffixes_path,
        "suffixes = [[\"unknown_letter\", \"unknown_letter\"]]\n",
    )
    .expect("write suffixes toml");

    let output = Command::new(exe)
        .arg("cross-loop")
        .arg("--spec")
        .arg(spec_path())
        .arg("--weight-min")
        .arg("2")
        .arg("--weight-max")
        .arg("2")
        .arg("--suffixes-toml")
        .arg(&suffixes_path)
        .arg("--out")
        .arg(&out_dir)
        .output()
        .expect("run mpl-experiments cross-loop scan");
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown suffix letter"));
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("experiments")
        .join("m2")
        .join("M2_toy_xy_unconstrained.toml")
}

fn unique_stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos()
}
