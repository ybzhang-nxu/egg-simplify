mod cli;
mod field;
mod linalg_modp;
mod spectral;
mod words;

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ExperimentError;
use field::{Field, Fp};
use linalg_modp::{dot, mat_vec_mul, vec_mat_mul, Matrix, Vector};
use spectral::SpectralModel;
use words::{LetterId, Word};

pub use cli::run_kze2_hankel_mvp_cli;

pub const ALPHABET: [&str; 5] = ["1", "11", "2", "22", "12"];
pub const ALPHABET_SIZE: usize = ALPHABET.len();

const MAX_LAMBDA_GAMMA_SHIFT: usize = 8;

#[derive(Clone, Copy)]
enum CandidateFamily {
    Lcg,
    GlobalLinearQuadratic,
    BlockMixed,
}

impl CandidateFamily {
    fn all() -> [Self; 3] {
        [Self::Lcg, Self::GlobalLinearQuadratic, Self::BlockMixed]
    }
}

#[derive(Clone, Debug)]
pub struct Kze2HankelMvpConfig {
    pub r: usize,
    pub prime: u64,
    pub prefix_len: usize,
    pub holdout_len: usize,
    pub out_dir: Option<PathBuf>,
}

impl Default for Kze2HankelMvpConfig {
    fn default() -> Self {
        Self {
            r: 20,
            prime: 1_000_003,
            prefix_len: 2,
            holdout_len: 6,
            out_dir: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Kze2HankelMvpReport {
    pub r: usize,
    pub hankel_rank: usize,
    pub prime: u64,
    pub prefix_len: usize,
    pub holdout_len: usize,
    pub w_train: usize,
    pub total_holdout_words: u64,
    pub mismatches: usize,
    pub out_dir: Option<PathBuf>,
}

pub fn run_kze2_hankel_mvp(
    cfg: &Kze2HankelMvpConfig,
) -> Result<Kze2HankelMvpReport, ExperimentError> {
    if cfg.r == 0 || !cfg.r.is_multiple_of(2) {
        return Err(ExperimentError::InvalidConfig(
            "r must be a positive even integer".to_string(),
        ));
    }
    if cfg.prefix_len == 0 {
        return Err(ExperimentError::InvalidConfig(
            "prefix_len must be >= 1".to_string(),
        ));
    }

    let field = Field::new(cfg.prime)?;
    let words = words::words_upto(cfg.prefix_len, ALPHABET_SIZE);
    if cfg.r > words.len() {
        return Err(ExperimentError::InvalidConfig(format!(
            "r={} exceeds max rank {} for prefix_len={}",
            cfg.r,
            words.len(),
            cfg.prefix_len
        )));
    }
    let w_train = cfg
        .prefix_len
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| ExperimentError::InvalidConfig("prefix_len overflow".to_string()))?;

    let matrices = build_kze2_matrices(cfg.r, &field)?;
    let (lambda, gamma, hankel, hankel_shifted, hankel_rank) =
        choose_lambda_gamma(&words, &matrices, &field, cfg.r)?;

    let model = spectral::spectral_learn(&hankel, &hankel_shifted, &field)?;
    if model.rank != hankel_rank {
        return Err(ExperimentError::InvalidConfig(format!(
            "learned rank {} does not match hankel rank {}",
            model.rank, hankel_rank
        )));
    }

    let total_holdout_words = words::count_words_exact(cfg.holdout_len, ALPHABET_SIZE)?;
    let mismatches =
        exhaustive_holdout(cfg.holdout_len, &matrices, &lambda, &gamma, &model, &field);

    let report = Kze2HankelMvpReport {
        r: cfg.r,
        hankel_rank,
        prime: cfg.prime,
        prefix_len: cfg.prefix_len,
        holdout_len: cfg.holdout_len,
        w_train,
        total_holdout_words,
        mismatches,
        out_dir: cfg.out_dir.clone(),
    };

    if let Some(out_dir) = report.out_dir.as_ref() {
        write_outputs(out_dir, cfg, &report)?;
    }

    Ok(report)
}

fn choose_lambda_gamma(
    words: &[Word],
    matrices: &[Matrix],
    field: &Field,
    target_rank: usize,
) -> Result<(Vector, Vector, Matrix, Vec<Matrix>, usize), ExperimentError> {
    let mut best_rank = 0usize;
    let mut best: Option<(Vector, Vector, Matrix, Vec<Matrix>, usize)> = None;
    for family in CandidateFamily::all() {
        for shift_lambda in 0..MAX_LAMBDA_GAMMA_SHIFT {
            for shift_gamma in 0..MAX_LAMBDA_GAMMA_SHIFT {
                let (lambda, gamma) = build_lambda_gamma(
                    field,
                    target_rank,
                    shift_lambda as u64,
                    shift_gamma as u64,
                    family,
                );
                let (hankel, hankel_shifted) =
                    build_hankel(words, matrices, &lambda, &gamma, field);
                let pivots = linalg_modp::pivot_columns(&hankel, field)?;
                let rank = pivots.len();
                if rank == target_rank {
                    return Ok((lambda, gamma, hankel, hankel_shifted, rank));
                }
                if rank > best_rank {
                    best_rank = rank;
                    best = Some((lambda, gamma, hankel, hankel_shifted, rank));
                }
            }
        }
    }
    if let Some(best) = best {
        return Ok(best);
    }
    let total_attempts = MAX_LAMBDA_GAMMA_SHIFT * MAX_LAMBDA_GAMMA_SHIFT * 3;
    Err(ExperimentError::InvalidConfig(format!(
        "failed to find nonzero rank after {total_attempts} deterministic attempts"
    )))
}

fn build_lambda_gamma(
    field: &Field,
    r: usize,
    shift_lambda: u64,
    shift_gamma: u64,
    family: CandidateFamily,
) -> (Vector, Vector) {
    let prime = field.prime() as u128;
    let shift_lambda = shift_lambda as u128;
    let shift_gamma = shift_gamma as u128;
    let mut lambda = Vec::with_capacity(r);
    let mut gamma = Vec::with_capacity(r);
    match family {
        CandidateFamily::Lcg => {
            let mut state = 0x9E3779B97F4A7C15u64 ^ (shift_lambda as u64);
            for _ in 0..r {
                state = lcg_step(state);
                lambda.push(field.reduce_u64(state));
            }
            let mut state = 0xD1B54A32D192ED03u64 ^ (shift_gamma as u64);
            for _ in 0..r {
                state = lcg_step(state);
                gamma.push(field.reduce_u64(state));
            }
        }
        CandidateFamily::GlobalLinearQuadratic => {
            for idx in 0..r {
                let i = idx as u128;
                let lambda_val = ((i + 1 + shift_lambda) % prime) as u64;
                let gamma_val = ((i * i + 1 + shift_gamma) % prime) as u64;
                lambda.push(field.reduce_u64(lambda_val));
                gamma.push(field.reduce_u64(gamma_val));
            }
        }
        CandidateFamily::BlockMixed => {
            let blocks = r / 2;
            for block in 0..blocks {
                let b = block as u128 + 1;
                let lambda0 = ((b + shift_lambda) % prime) as u64;
                let lambda1 = ((b * (b + 1) + shift_lambda) % prime) as u64;
                let gamma0 = ((b * b + 1 + shift_gamma) % prime) as u64;
                let gamma1 = ((b * b * b + 1 + shift_gamma) % prime) as u64;
                lambda.push(field.reduce_u64(lambda0));
                lambda.push(field.reduce_u64(lambda1));
                gamma.push(field.reduce_u64(gamma0));
                gamma.push(field.reduce_u64(gamma1));
            }
        }
    }
    (lambda, gamma)
}

fn lcg_step(state: u64) -> u64 {
    state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

fn build_hankel(
    words: &[Word],
    matrices: &[Matrix],
    lambda: &[Fp],
    gamma: &[Fp],
    field: &Field,
) -> (Matrix, Vec<Matrix>) {
    let prefix_rows = compute_prefix_rows(words, matrices, lambda, field);
    let suffix_cols = compute_suffix_cols(words, matrices, gamma, field);

    let mut hankel = linalg_modp::zeros(words.len(), words.len(), field);
    for (row_idx, row) in prefix_rows.iter().enumerate() {
        for (col_idx, col) in suffix_cols.iter().enumerate() {
            hankel[row_idx][col_idx] = dot(row, col, field);
        }
    }

    let mut shifted_rows = vec![vec![Vec::new(); ALPHABET_SIZE]; words.len()];
    for (row_idx, row) in prefix_rows.iter().enumerate() {
        for (letter, slot) in shifted_rows[row_idx].iter_mut().enumerate() {
            *slot = vec_mat_mul(row, &matrices[letter], field);
        }
    }

    let mut hankel_shifted = Vec::with_capacity(ALPHABET_SIZE);
    for (letter, _) in shifted_rows[0].iter().enumerate() {
        let mut ha = linalg_modp::zeros(words.len(), words.len(), field);
        for row_idx in 0..words.len() {
            let row = &shifted_rows[row_idx][letter];
            for (col_idx, col) in suffix_cols.iter().enumerate() {
                ha[row_idx][col_idx] = dot(row, col, field);
            }
        }
        hankel_shifted.push(ha);
    }

    (hankel, hankel_shifted)
}

fn compute_prefix_rows(
    words: &[Word],
    matrices: &[Matrix],
    lambda: &[Fp],
    field: &Field,
) -> Vec<Vector> {
    words
        .iter()
        .map(|word| {
            let mut row = lambda.to_vec();
            for &letter in word {
                row = vec_mat_mul(&row, &matrices[letter as usize], field);
            }
            row
        })
        .collect()
}

fn compute_suffix_cols(
    words: &[Word],
    matrices: &[Matrix],
    gamma: &[Fp],
    field: &Field,
) -> Vec<Vector> {
    words
        .iter()
        .map(|word| {
            let mut col = gamma.to_vec();
            for &letter in word.iter().rev() {
                col = mat_vec_mul(&matrices[letter as usize], &col, field);
            }
            col
        })
        .collect()
}

fn exhaustive_holdout(
    holdout_len: usize,
    matrices: &[Matrix],
    lambda: &[Fp],
    gamma: &[Fp],
    model: &SpectralModel,
    field: &Field,
) -> usize {
    let mut mismatches = 0usize;
    words::for_each_word_len(holdout_len, ALPHABET_SIZE, |word| {
        let pred = eval_wfa(&model.alpha, &model.beta, &model.transitions, word, field);
        let truth = eval_true(lambda, gamma, matrices, word, field);
        if pred != truth {
            mismatches += 1;
        }
    });
    mismatches
}

fn eval_true(
    lambda: &[Fp],
    gamma: &[Fp],
    matrices: &[Matrix],
    word: &[LetterId],
    field: &Field,
) -> Fp {
    let mut state = lambda.to_vec();
    for &letter in word {
        state = vec_mat_mul(&state, &matrices[letter as usize], field);
    }
    dot(&state, gamma, field)
}

fn eval_wfa(
    alpha: &[Fp],
    beta: &[Fp],
    transitions: &[Matrix],
    word: &[LetterId],
    field: &Field,
) -> Fp {
    let mut state = alpha.to_vec();
    for &letter in word {
        state = vec_mat_mul(&state, &transitions[letter as usize], field);
    }
    dot(&state, beta, field)
}

fn build_kze2_matrices(r: usize, field: &Field) -> Result<Vec<Matrix>, ExperimentError> {
    if r == 0 || !r.is_multiple_of(2) {
        return Err(ExperimentError::InvalidConfig(
            "r must be a positive even integer".to_string(),
        ));
    }
    let blocks = base_blocks();
    let blocks_per_matrix = r / 2;
    let mut out = Vec::with_capacity(ALPHABET_SIZE);
    for block in blocks.iter() {
        let mut mat = linalg_modp::zeros(r, r, field);
        for block_idx in 0..blocks_per_matrix {
            let scale = field.reduce_u64((block_idx + 1) as u64);
            let row_base = block_idx * 2;
            for r_idx in 0..2 {
                for c_idx in 0..2 {
                    let value = field.reduce_i64(block[r_idx][c_idx]);
                    mat[row_base + r_idx][row_base + c_idx] = field.mul(scale, value);
                }
            }
        }
        out.push(mat);
    }
    Ok(out)
}

fn base_blocks() -> [[[i64; 2]; 2]; ALPHABET_SIZE] {
    [
        [[0, 0], [0, 0]],
        [[0, 0], [0, 1]],
        [[1, 0], [0, 0]],
        [[0, 1], [1, 0]],
        [[0, -1], [-1, -1]],
    ]
}

fn write_outputs(
    out_dir: &Path,
    cfg: &Kze2HankelMvpConfig,
    report: &Kze2HankelMvpReport,
) -> Result<(), ExperimentError> {
    fs::create_dir_all(out_dir)?;

    let params = ParamsOutput {
        alphabet: ALPHABET.iter().map(|name| (*name).to_string()).collect(),
        r: cfg.r,
        prime: cfg.prime,
        prefix_len: cfg.prefix_len,
        holdout_len: cfg.holdout_len,
    };
    let params_json = serde_json::to_string_pretty(&params).map_err(|err| {
        ExperimentError::InvalidConfig(format!("params.json encode error: {err}"))
    })?;
    fs::write(out_dir.join("params.json"), params_json)?;

    let stats_txt = format!(
        "r = {}\nprime = {}\nL = {}\nW_train = {}\nholdout_len = {}\ntotal_holdout_words = {}\nhankel_rank = {}\nmismatches = {}\n",
        report.r,
        report.prime,
        report.prefix_len,
        report.w_train,
        report.holdout_len,
        report.total_holdout_words,
        report.hankel_rank,
        report.mismatches
    );
    fs::write(out_dir.join("stats.txt"), stats_txt)?;

    Ok(())
}

#[derive(Serialize)]
struct ParamsOutput {
    alphabet: Vec<String>,
    r: usize,
    prime: u64,
    prefix_len: usize,
    holdout_len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kze2_hankel_mvp_holdout_matches() {
        let cfg = Kze2HankelMvpConfig {
            r: 10,
            prime: 1_000_003,
            prefix_len: 3,
            holdout_len: 6,
            out_dir: None,
        };
        let report = run_kze2_hankel_mvp(&cfg).expect("run kze2 hankel mvp");
        assert_eq!(report.mismatches, 0);
    }
}
