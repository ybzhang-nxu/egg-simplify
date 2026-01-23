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
fn esymb_span_deps_cli_writes_outputs() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let input = fixture_path();
    assert!(input.exists(), "fixture not found at {}", input.display());

    let out_dir = std::env::temp_dir().join(format!("mpl_span_deps_cli_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let status = Command::new(exe)
        .arg("esymb-span-deps")
        .arg("--in")
        .arg(&input)
        .arg("--export-equiv-classes")
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments esymb-span-deps");
    assert!(status.success());

    let stats_path = out_dir.join("span_stats.csv");
    let equiv_path = out_dir.join("equiv_classes.csv");
    let deps_path = out_dir.join("span_deps.csv");
    let md_path = out_dir.join("span_deps.md");

    assert!(stats_path.exists());
    assert!(equiv_path.exists());
    assert!(deps_path.exists());
    assert!(md_path.exists());

    let stats = fs::read_to_string(stats_path).expect("read span_stats.csv");
    assert!(stats.contains("prefix,6,5,1,2,3"));

    let equiv = fs::read_to_string(equiv_path).expect("read equiv_classes.csv");
    assert!(equiv.contains("prefix,\"prefix|r=1,p=a\",\"prefix|r=1,p=c\",2"));

    let deps = fs::read_to_string(deps_path).expect("read span_deps.csv");
    assert!(
        deps.contains("prefix,3,\"1*prefix|r=1,p=a\",\"1*prefix|r=1,p=d\",\"-1*prefix|r=1,p=e\"")
    );

    let md = fs::read_to_string(md_path).expect("read span_deps.md");
    assert!(md.contains("loops = [1, 2]"));
    assert!(md.contains("## family_stats"));
}

#[test]
fn esymb_span_deps_exports_forbidden_and_equiv_classes() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let input = fixture_equiv_path();
    assert!(input.exists(), "fixture not found at {}", input.display());

    let out_dir = std::env::temp_dir().join(format!("mpl_span_deps_equiv_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let status = Command::new(exe)
        .arg("esymb-span-deps")
        .arg("--observables")
        .arg(&input)
        .arg("--export-forbidden")
        .arg("--export-equiv-classes")
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments esymb-span-deps");
    assert!(status.success());

    let forbidden_path = out_dir.join("forbidden_keys.csv");
    let nonzero_path = out_dir.join("nonzero_keys.csv");
    let equiv_path = out_dir.join("equiv_classes.csv");

    assert!(forbidden_path.exists());
    assert!(nonzero_path.exists());
    assert!(equiv_path.exists());

    let forbidden = fs::read_to_string(forbidden_path).expect("read forbidden_keys.csv");
    let forbidden_count = forbidden
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .count();
    assert_eq!(forbidden_count, 2);

    let nonzero = fs::read_to_string(nonzero_path).expect("read nonzero_keys.csv");
    let nonzero_count = nonzero
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .count();
    assert_eq!(nonzero_count, 3);

    let equiv = fs::read_to_string(equiv_path).expect("read equiv_classes.csv");
    assert!(equiv.contains("prefix,\"prefix|r=1,p=a\",\"prefix|r=1,p=b\",2"));
    assert!(equiv.contains("prefix,\"prefix|r=1,p=a\",\"prefix|r=1,p=c\",-1"));
}

#[test]
fn esymb_span_deps_pm2_support3_finds_sum_relation() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let input = fixture_pm2_path();
    assert!(input.exists(), "fixture not found at {}", input.display());

    let out_dir = std::env::temp_dir().join(format!("mpl_span_deps_pm2_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let status = Command::new(exe)
        .arg("esymb-span-deps")
        .arg("--observables")
        .arg(&input)
        .arg("--coef-set")
        .arg("pm2")
        .arg("--support-max")
        .arg("3")
        .arg("--top-k")
        .arg("50")
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments esymb-span-deps pm2");
    assert!(status.success());

    let relations = fs::read_to_string(out_dir.join("relations.csv")).expect("read relations");
    assert!(relations
        .contains("prefix,3,\"1*prefix|r=1,p=a\",\"1*prefix|r=1,p=b\",\"-1*prefix|r=1,p=c\""));
}

#[test]
fn esymb_span_deps_pm2_support4_finds_sum_relation() {
    let exe = bin_path();
    assert!(exe.exists(), "binary not found at {}", exe.display());

    let input = fixture_pm2_support4_path();
    assert!(input.exists(), "fixture not found at {}", input.display());

    let out_dir = std::env::temp_dir().join(format!("mpl_span_deps_pm2_s4_{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let status = Command::new(exe)
        .arg("esymb-span-deps")
        .arg("--observables")
        .arg(&input)
        .arg("--coef-set")
        .arg("pm2")
        .arg("--support-max")
        .arg("4")
        .arg("--top-k")
        .arg("50")
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .expect("run mpl-experiments esymb-span-deps pm2 support4");
    assert!(status.success());

    let relations = fs::read_to_string(out_dir.join("relations.csv")).expect("read relations");
    assert!(relations.contains(
        "prefix,4,\"1*prefix|r=1,p=a\",\"1*prefix|r=1,p=b\",\"1*prefix|r=1,p=c\",\"-1*prefix|r=1,p=d\""
    ));
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("span_deps_golden.csv")
}

fn fixture_equiv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("span_deps_equiv.csv")
}

fn fixture_pm2_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("span_deps_pm2_support3.csv")
}

fn fixture_pm2_support4_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("span_deps_pm2_support4.csv")
}
