use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
fn esymb_hankel_subblock_cli_writes_outputs() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let input = fixture_path();
    assert!(input.exists(), "fixture not found at {}", input.display());

    let out_dir =
        std::env::temp_dir().join(format!("mpl_hankel_subblock_cli_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let status = Command::new(exe)
        .arg("esymb-hankel-subblock")
        .arg("--in")
        .arg(&input)
        .arg("--r")
        .arg("1")
        .arg("--k")
        .arg("1")
        .arg("--exact")
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments esymb-hankel-subblock");
    assert!(status.success());

    let stats_path = out_dir.join("hankel_subblock_stats.csv");
    let row_deps_path = out_dir.join("hankel_row_deps.csv");
    let col_deps_path = out_dir.join("hankel_col_deps.csv");
    let md_path = out_dir.join("hankel_subblock.md");

    assert!(stats_path.exists());
    assert!(row_deps_path.exists());
    assert!(col_deps_path.exists());
    assert!(md_path.exists());

    let stats = fs::read_to_string(stats_path).expect("read stats");
    assert!(stats.contains("2,1,1,2,2,1"));
    assert!(stats.contains("3,1,1,2,2,1"));

    let row_deps = fs::read_to_string(row_deps_path).expect("read row deps");
    assert!(row_deps.contains("2,1,1,1000003,u=b,1*u=a"));

    let col_deps = fs::read_to_string(col_deps_path).expect("read col deps");
    assert!(col_deps.contains("2,1,1,1000003,v=d,2*v=c"));

    let md = fs::read_to_string(md_path).expect("read md");
    assert!(md.contains("rank_summary"));
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("hankel_subblock_golden.csv")
}
