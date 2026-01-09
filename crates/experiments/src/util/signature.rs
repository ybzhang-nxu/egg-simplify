use std::fs;
use std::path::{Path, PathBuf};

use crate::util::sanitize::sanitize_layer_name;
use crate::ExperimentError;

const FULL_RUN_FILES: &[&str] = &[
    "basis_stats.txt",
    "dim_vs_w.csv",
    "pairs.csv",
    "pairs_by_weight.csv",
    "triplets.csv",
    "triplets_by_weight.csv",
    "topology_metrics.csv",
    "skeleton2_metrics.csv",
];
const COUNT_ONLY_FILES: &[&str] = &["counts_only.csv"];

pub(crate) fn read_signature(out_dir: &Path, full_run: bool) -> Result<String, ExperimentError> {
    let files = if full_run {
        FULL_RUN_FILES
    } else {
        COUNT_ONLY_FILES
    };
    build_signature(out_dir, files)
}

pub(crate) fn filtration_temp_dir(
    spec_name: &str,
    layer_index: usize,
    weight: usize,
    repeat_idx: usize,
) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("mpl_experiments_filtration");
    path.push(sanitize_layer_name(spec_name));
    path.push(format!("{layer_index}_{weight}_{repeat_idx}"));
    path
}

fn build_signature(out_dir: &Path, files: &[&str]) -> Result<String, ExperimentError> {
    let mut ordered: Vec<&str> = files.to_vec();
    ordered.sort_unstable();
    let mut out = String::new();
    for file in ordered {
        let content = fs::read_to_string(out_dir.join(file)).map_err(ExperimentError::Io)?;
        out.push_str(file);
        out.push('\n');
        out.push_str(&normalize_newlines(&content));
        out.push('\n');
    }
    Ok(out)
}

fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}
