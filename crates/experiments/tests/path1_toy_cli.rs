use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
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

fn collect_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .expect("strip prefix")
                .to_string_lossy()
                .replace('\\', "/");
            let data = fs::read(&path).expect("read file");
            out.insert(rel, data);
        }
    }
    out
}

#[test]
fn path1_toy_oracle_is_deterministic() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let out_a = std::env::temp_dir().join(format!("mpl_path1_oracle_a_{}", std::process::id()));
    let out_b = std::env::temp_dir().join(format!("mpl_path1_oracle_b_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_a);
    let _ = fs::remove_dir_all(&out_b);

    let status = Command::new(&exe)
        .arg("path1-toy")
        .arg("--mode")
        .arg("oracle")
        .arg("--weights")
        .arg("1..4")
        .arg("--export-jsonl")
        .arg("--out-dir")
        .arg(&out_a)
        .status()
        .expect("run path1-toy oracle a");
    assert!(status.success());

    let status = Command::new(&exe)
        .arg("path1-toy")
        .arg("--mode")
        .arg("oracle")
        .arg("--weights")
        .arg("1..4")
        .arg("--export-jsonl")
        .arg("--out-dir")
        .arg(&out_b)
        .status()
        .expect("run path1-toy oracle b");
    assert!(status.success());

    let files_a = collect_files(&out_a);
    let files_b = collect_files(&out_b);
    assert!(!files_a.is_empty());
    assert_eq!(files_a, files_b);

    let expected = out_a.join("oracle").join("basis_stats.csv");
    assert!(expected.exists());
}

#[test]
fn path1_toy_scaled_is_deterministic() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let out_a = std::env::temp_dir().join(format!("mpl_path1_scaled_a_{}", std::process::id()));
    let out_b = std::env::temp_dir().join(format!("mpl_path1_scaled_b_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_a);
    let _ = fs::remove_dir_all(&out_b);

    let status = Command::new(&exe)
        .arg("path1-toy")
        .arg("--mode")
        .arg("scaled")
        .arg("--loops")
        .arg("1..3")
        .arg("--max-alternations")
        .arg("1")
        .arg("--max-words")
        .arg("1000")
        .arg("--out-dir")
        .arg(&out_a)
        .status()
        .expect("run path1-toy scaled a");
    assert!(status.success());

    let status = Command::new(&exe)
        .arg("path1-toy")
        .arg("--mode")
        .arg("scaled")
        .arg("--loops")
        .arg("1..3")
        .arg("--max-alternations")
        .arg("1")
        .arg("--max-words")
        .arg("1000")
        .arg("--out-dir")
        .arg(&out_b)
        .status()
        .expect("run path1-toy scaled b");
    assert!(status.success());

    let files_a = collect_files(&out_a);
    let files_b = collect_files(&out_b);
    assert!(!files_a.is_empty());
    assert_eq!(files_a, files_b);

    let expected = out_a.join("scaled").join("loop_stats.csv");
    assert!(expected.exists());
}
