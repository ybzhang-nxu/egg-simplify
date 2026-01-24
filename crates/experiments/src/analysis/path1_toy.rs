use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use mpl_symbol::space::{
    build_integrable_basis_with_acceptor_with_stats, build_integrable_basis_with_stats,
    check_integrable_n, Alphabet, Basis, BasisStats, ConstraintBudget, MaxAlternationsAcceptor,
    WordConstraints,
};
use mpl_symbol::{Coeff, Symbol, Word};
use num_traits::Zero;
use serde::Serialize;

use crate::analysis::esymb_hankel_subblock::{
    run_esymb_hankel_subblock, EsymbHankelSubblockConfig,
};
use crate::analysis::esymb_rank_scan::{
    run_esymb_rank_scan, AlphabetMode, EsymbRankScanConfig, NormalizeChoice, PairsMode,
};
use crate::analysis::esymb_span_deps::{
    run_esymb_span_deps, CoefSet, EsymbSpanDepsConfig, SpanFamilyFilter,
};
use crate::build::alphabet::{letter_display_names, toy_alphabet_xy};
use crate::output::csv::CsvWriter;
use crate::ExperimentError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path1Mode {
    Oracle,
    Scaled,
}

impl Path1Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Oracle => "oracle",
            Self::Scaled => "scaled",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Path1ToyConfig {
    pub mode: Path1Mode,
    pub out_dir: PathBuf,
    pub weights: Vec<usize>,
    pub loops: Vec<usize>,
    pub max_words: u64,
    pub max_alternations: usize,
    pub max_terms: Option<usize>,
    pub export_oracle_jsonl: bool,
    pub run_esymb: bool,
}

#[derive(Clone, Debug)]
pub struct Path1ToyReport {
    pub mode: Path1Mode,
    pub out_dir: PathBuf,
    pub weights: Vec<usize>,
    pub loops: Vec<usize>,
    pub ran_esymb: bool,
}

pub fn run_path1_toy(cfg: &Path1ToyConfig) -> Result<Path1ToyReport, ExperimentError> {
    if cfg.max_words == 0 {
        return Err(ExperimentError::InvalidConfig(
            "max_words must be >= 1".to_string(),
        ));
    }
    fs::create_dir_all(&cfg.out_dir)?;

    let mut report = Path1ToyReport {
        mode: cfg.mode,
        out_dir: cfg.out_dir.clone(),
        weights: Vec::new(),
        loops: Vec::new(),
        ran_esymb: false,
    };

    match cfg.mode {
        Path1Mode::Oracle => {
            report.weights = run_oracle(cfg)?;
        }
        Path1Mode::Scaled => {
            let (loops, ran_esymb) = run_scaled(cfg)?;
            report.loops = loops;
            report.ran_esymb = ran_esymb;
        }
    }

    fs::write(cfg.out_dir.join("SUMMARY.md"), render_summary(cfg, &report))?;
    Ok(report)
}

fn run_oracle(cfg: &Path1ToyConfig) -> Result<Vec<usize>, ExperimentError> {
    if cfg.weights.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "oracle mode requires at least one weight".to_string(),
        ));
    }

    let oracle_dir = cfg.out_dir.join("oracle");
    fs::create_dir_all(&oracle_dir)?;

    let mut weights = cfg.weights.clone();
    weights.sort_unstable();
    weights.dedup();

    let mut stats_csv = CsvWriter::new();
    stats_csv.push_record([
        "weight",
        "ncols",
        "dim",
        "rank",
        "rows_attempted",
        "rows_inserted",
        "samples_used",
        "envs_total",
        "sample_table",
        "rows_skipped_singular",
        "constraints_insufficient_samples",
        "vars",
        "max_row_nnz",
        "avg_row_nnz",
    ]);

    let mut checks_csv = CsvWriter::new();
    checks_csv.push_record([
        "weight",
        "expected_dim",
        "actual_dim",
        "status",
        "integrable",
        "terms",
        "basis_vectors_used",
    ]);

    let alpha = toy_alphabet_xy();
    let constraints = WordConstraints::default();
    let letter_names = letter_display_names(&alpha);
    let jsonl_dir = if cfg.export_oracle_jsonl {
        let dir = oracle_dir.join("symbols_jsonl");
        fs::create_dir_all(&dir)?;
        Some(dir)
    } else {
        None
    };

    for weight in &weights {
        let expected_words = predict_unconstrained_words(*weight).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!("weight {weight} overflow for 2^w"))
        })?;
        if expected_words > cfg.max_words {
            return Err(ExperimentError::InvalidConfig(format!(
                "weight {weight} has 2^w={expected_words} words (max_words={})",
                cfg.max_words
            )));
        }

        let basis = build_integrable_basis_with_stats(&alpha, &constraints, *weight)
            .map_err(map_basis_error)?;
        let stats = basis.stats();
        if stats.dim != weight.saturating_add(1) {
            return Err(ExperimentError::InvalidConfig(format!(
                "oracle mismatch at weight {weight}: dim={} expected={}",
                stats.dim,
                weight + 1
            )));
        }
        if stats.ncols as u64 != expected_words {
            return Err(ExperimentError::InvalidConfig(format!(
                "oracle word count mismatch at weight {weight}: ncols={} expected={}",
                stats.ncols, expected_words
            )));
        }

        let seed = seed_from_params(Path1Mode::Oracle, *weight, None);
        let synth = synthesize_symbol(&basis, &alpha, seed, cfg.max_terms)?;
        let integrable = check_integrable_n(&synth.symbol)?;
        if !integrable {
            return Err(ExperimentError::InvalidConfig(format!(
                "oracle synthesized symbol not integrable at weight {weight}"
            )));
        }

        if let Some(dir) = &jsonl_dir {
            let path = dir.join(format!("Esymb_L{weight}.jsonl"));
            write_esymb_jsonl(
                &path,
                "Path1Toy",
                *weight,
                synth.terms,
                &basis.words,
                &synth.values,
                &letter_names,
            )?;
        }

        push_basis_stats_row(&mut stats_csv, *weight, None, stats);
        let status = if stats.dim == weight + 1 { "pass" } else { "fail" };
        checks_csv.push_record([
            weight.to_string(),
            (weight + 1).to_string(),
            stats.dim.to_string(),
            status.to_string(),
            integrable.to_string(),
            synth.terms.to_string(),
            synth.basis_vectors_used.to_string(),
        ]);
    }

    fs::write(oracle_dir.join("basis_stats.csv"), stats_csv.into_string())?;
    fs::write(oracle_dir.join("oracle_checks.csv"), checks_csv.into_string())?;

    Ok(weights)
}

fn run_scaled(cfg: &Path1ToyConfig) -> Result<(Vec<usize>, bool), ExperimentError> {
    if cfg.loops.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "scaled mode requires at least one loop".to_string(),
        ));
    }

    let scaled_dir = cfg.out_dir.join("scaled");
    let jsonl_dir = scaled_dir.join("esymb_jsonl");
    fs::create_dir_all(&jsonl_dir)?;

    let mut loops = cfg.loops.clone();
    loops.sort_unstable();
    loops.dedup();

    let alpha = toy_alphabet_xy();
    if alpha.letters.len() != 2 {
        return Err(ExperimentError::InvalidConfig(
            "scaled mode expects toy alphabet size 2".to_string(),
        ));
    }
    let letter_names = letter_display_names(&alpha);
    let acceptor = MaxAlternationsAcceptor::new(cfg.max_alternations);
    let budget = ConstraintBudget {
        max_words: Some(cfg.max_words),
        ..Default::default()
    };

    let mut basis_stats_csv = CsvWriter::new();
    basis_stats_csv.push_record([
        "loop",
        "weight",
        "ncols",
        "dim",
        "rank",
        "rows_attempted",
        "rows_inserted",
        "samples_used",
        "envs_total",
        "sample_table",
        "rows_skipped_singular",
        "constraints_insufficient_samples",
        "vars",
        "max_row_nnz",
        "avg_row_nnz",
    ]);

    let mut loop_stats_csv = CsvWriter::new();
    loop_stats_csv.push_record([
        "loop",
        "weight",
        "alphabet_size",
        "words",
        "dim",
        "terms",
        "basis_vectors_used",
        "max_alternations",
    ]);

    for loop_value in &loops {
        let weight = loop_value
            .checked_mul(2)
            .ok_or_else(|| ExperimentError::InvalidConfig("loop * 2 overflow".to_string()))?;
        let predicted = predict_max_alternations_words(weight, cfg.max_alternations).ok_or_else(
            || ExperimentError::InvalidConfig(format!("weight {weight} word count overflow")),
        )?;
        if predicted > cfg.max_words {
            return Err(ExperimentError::InvalidConfig(format!(
                "loop {loop_value} weight {weight} has {predicted} words (max_words={})",
                cfg.max_words
            )));
        }

        let basis =
            build_integrable_basis_with_acceptor_with_stats(&alpha, &acceptor, weight, Some(&budget))
                .map_err(map_basis_error)?;
        let stats = basis.stats();

        let seed = seed_from_params(Path1Mode::Scaled, weight, Some(cfg.max_alternations));
        let synth = synthesize_symbol(&basis, &alpha, seed, cfg.max_terms)?;
        let integrable = check_integrable_n(&synth.symbol)?;
        if !integrable {
            return Err(ExperimentError::InvalidConfig(format!(
                "scaled symbol not integrable at loop {loop_value} weight {weight}"
            )));
        }

        let jsonl_path = jsonl_dir.join(format!("Esymb_L{loop_value}.jsonl"));
        write_esymb_jsonl(
            &jsonl_path,
            "Path1Toy",
            *loop_value,
            synth.terms,
            &basis.words,
            &synth.values,
            &letter_names,
        )?;

        push_basis_stats_row(&mut basis_stats_csv, weight, Some(*loop_value), stats);
        loop_stats_csv.push_record([
            loop_value.to_string(),
            weight.to_string(),
            alpha.letters.len().to_string(),
            stats.ncols.to_string(),
            stats.dim.to_string(),
            synth.terms.to_string(),
            synth.basis_vectors_used.to_string(),
            cfg.max_alternations.to_string(),
        ]);
    }

    fs::write(scaled_dir.join("basis_stats.csv"), basis_stats_csv.into_string())?;
    fs::write(scaled_dir.join("loop_stats.csv"), loop_stats_csv.into_string())?;

    let mut ran_esymb = false;
    if cfg.run_esymb {
        let min_loop = loops.iter().copied().min().unwrap_or(0);
        if min_loop < 2 {
            return Err(ExperimentError::InvalidConfig(
                "run-esymb requires loops >= 2 for prefix-suffix r=2,k=2".to_string(),
            ));
        }
        run_esymb_pipeline(&scaled_dir, &jsonl_dir, &loops)?;
        ran_esymb = true;
    }

    Ok((loops, ran_esymb))
}

fn run_esymb_pipeline(
    scaled_dir: &Path,
    jsonl_dir: &Path,
    loops: &[usize],
) -> Result<(), ExperimentError> {
    let rank_out = scaled_dir.join("esymb_rank_scan");
    let span_out = scaled_dir.join("esymb_span_deps");
    let hankel_out = scaled_dir.join("esymb_hankel_subblock");
    let primes = vec![1000003, 1000033, 1000037];

    let rank_cfg = EsymbRankScanConfig {
        data_dir: Some(jsonl_dir.to_path_buf()),
        glob: None,
        loops: loops.to_vec(),
        family_pow_last: false,
        family_block2: false,
        family_prefix: false,
        family_suffix: false,
        family_prefix_suffix: true,
        x_set: Vec::new(),
        y_set: Vec::new(),
        pairs: Vec::new(),
        alphabet: vec!["x".to_string(), "y".to_string()],
        alphabet_project: false,
        prefix_len: Some(2),
        suffix_len: Some(2),
        only_observed: false,
        validate_marginals: false,
        export_observables: true,
        matrix_rank: true,
        r_budget: 6,
        primes: primes.clone(),
        float_rank: false,
        float_tau: 1e-12,
        subsample_rank: false,
        subsample_size: 4,
        seed: 0,
        plateau_len: 2,
        normalize: NormalizeChoice::Auto,
        skip_trivial: true,
        alphabet_mode: AlphabetMode::Manual,
        pairs_mode: PairsMode::Manual,
        attempt_solve_inconclusive: false,
        out_dir: rank_out.clone(),
    };
    run_esymb_rank_scan(&rank_cfg)?;

    let observables = rank_out.join("marginals_observables.csv");
    let span_cfg = EsymbSpanDepsConfig {
        input: observables.clone(),
        out_dir: span_out.clone(),
        family: SpanFamilyFilter::PrefixSuffix,
        support_max: 3,
        coef_set: CoefSet::Pm1,
        top_k: 200,
        export_forbidden: false,
        export_equiv_classes: false,
    };
    run_esymb_span_deps(&span_cfg)?;

    let hankel_cfg = EsymbHankelSubblockConfig {
        input: observables,
        out_dir: hankel_out,
        r: 2,
        k: 2,
        loops: Some(loops.to_vec()),
        primes,
        exact: true,
    };
    run_esymb_hankel_subblock(&hankel_cfg)?;
    Ok(())
}

fn push_basis_stats_row(
    writer: &mut CsvWriter,
    weight: usize,
    loop_value: Option<usize>,
    stats: &BasisStats,
) {
    let avg_row_nnz = if stats.rows_inserted == 0 {
        0
    } else {
        stats.sum_row_nnz / stats.rows_inserted
    };
    if let Some(loop_value) = loop_value {
        writer.push_record([
            loop_value.to_string(),
            weight.to_string(),
            stats.ncols.to_string(),
            stats.dim.to_string(),
            stats.rank.to_string(),
            stats.rows_attempted.to_string(),
            stats.rows_inserted.to_string(),
            stats.samples_used.to_string(),
            stats.envs_total.to_string(),
            stats.sample_table.as_str().to_string(),
            stats.rows_skipped_singular.to_string(),
            stats.constraints_insufficient_samples.to_string(),
            stats.vars_count.to_string(),
            stats.max_row_nnz.to_string(),
            avg_row_nnz.to_string(),
        ]);
    } else {
        writer.push_record([
            weight.to_string(),
            stats.ncols.to_string(),
            stats.dim.to_string(),
            stats.rank.to_string(),
            stats.rows_attempted.to_string(),
            stats.rows_inserted.to_string(),
            stats.samples_used.to_string(),
            stats.envs_total.to_string(),
            stats.sample_table.as_str().to_string(),
            stats.rows_skipped_singular.to_string(),
            stats.constraints_insufficient_samples.to_string(),
            stats.vars_count.to_string(),
            stats.max_row_nnz.to_string(),
            avg_row_nnz.to_string(),
        ]);
    }
}

fn map_basis_error(err: mpl_symbol::space::BasisBuildError) -> ExperimentError {
    ExperimentError::InvalidConfig(format!("basis build error: {err}"))
}

struct SynthResult {
    values: Vec<Coeff>,
    terms: usize,
    basis_vectors_used: usize,
    symbol: Symbol,
}

fn synthesize_symbol(
    basis: &Basis,
    alpha: &Alphabet,
    seed: u64,
    max_terms: Option<usize>,
) -> Result<SynthResult, ExperimentError> {
    let ncols = basis.words.len();
    let mut values = vec![Coeff::zero(); ncols];
    let mut nonzero = 0usize;
    let mut used_vectors = 0usize;

    let mut rng = SplitMix64::new(seed);
    let coeffs: Vec<Coeff> = (0..basis.vectors.len())
        .map(|_| coeff_from_rng(&mut rng))
        .collect();

    for (vec_idx, vec) in basis.vectors.iter().enumerate() {
        let coeff = coeffs[vec_idx];
        if coeff.is_zero() {
            continue;
        }
        if vec.len() != ncols {
            return Err(ExperimentError::InvalidConfig(
                "basis vector length mismatch".to_string(),
            ));
        }
        if let Some(limit) = max_terms {
            let prev_nonzero = nonzero;
            let mut changes: Vec<(usize, Coeff)> = Vec::new();
            for col in 0..ncols {
                let add = coeff * vec[col];
                if add.is_zero() {
                    continue;
                }
                let prev = values[col];
                let next = prev + add;
                if prev == next {
                    continue;
                }
                changes.push((col, prev));
                if prev.is_zero() && !next.is_zero() {
                    nonzero = nonzero.saturating_add(1);
                } else if !prev.is_zero() && next.is_zero() {
                    nonzero = nonzero.saturating_sub(1);
                }
                values[col] = next;
            }
            if nonzero > limit {
                for (col, prev) in changes {
                    values[col] = prev;
                }
                nonzero = prev_nonzero;
                break;
            }
            used_vectors = used_vectors.saturating_add(1);
        } else {
            for col in 0..ncols {
                let add = coeff * vec[col];
                if add.is_zero() {
                    continue;
                }
                let prev = values[col];
                let next = prev + add;
                if prev == next {
                    continue;
                }
                if prev.is_zero() && !next.is_zero() {
                    nonzero = nonzero.saturating_add(1);
                } else if !prev.is_zero() && next.is_zero() {
                    nonzero = nonzero.saturating_sub(1);
                }
                values[col] = next;
            }
            used_vectors = used_vectors.saturating_add(1);
        }
    }

    if let Some(limit) = max_terms {
        if used_vectors == 0 && !basis.vectors.is_empty() {
            return Err(ExperimentError::InvalidConfig(format!(
                "max_terms {limit} too small to include first basis vector"
            )));
        }
    }

    let symbol = symbol_from_values(alpha, &basis.words, &values);
    Ok(SynthResult {
        values,
        terms: nonzero,
        basis_vectors_used: used_vectors,
        symbol,
    })
}

fn symbol_from_values(alpha: &Alphabet, words: &[Vec<usize>], values: &[Coeff]) -> Symbol {
    let mut terms = Vec::new();
    for (ids, coeff) in words.iter().zip(values.iter()) {
        if coeff.is_zero() {
            continue;
        }
        let word = Word(ids.iter().map(|&idx| alpha.letters[idx].clone()).collect());
        terms.push((word, *coeff));
    }
    Symbol::from_terms(terms)
}

#[derive(Serialize)]
struct MetaLine<'a> {
    #[serde(rename = "_meta")]
    meta: MetaContent<'a>,
}

#[derive(Serialize)]
struct MetaContent<'a> {
    name: &'a str,
    #[serde(rename = "loop")]
    loop_index: usize,
    merged_terms: usize,
}

#[derive(Serialize)]
struct TermLine {
    word: Vec<String>,
    coeff: String,
}

fn write_esymb_jsonl(
    path: &Path,
    name: &str,
    loop_index: usize,
    merged_terms: usize,
    words: &[Vec<usize>],
    values: &[Coeff],
    letter_names: &[String],
) -> Result<(), ExperimentError> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    let meta = MetaLine {
        meta: MetaContent {
            name,
            loop_index,
            merged_terms,
        },
    };
    let meta_line = serde_json::to_string(&meta).map_err(|err| {
        ExperimentError::InvalidConfig(format!("json encode error: {err}"))
    })?;
    writer.write_all(meta_line.as_bytes())?;
    writer.write_all(b"\n")?;

    for (ids, coeff) in words.iter().zip(values.iter()) {
        if coeff.is_zero() {
            continue;
        }
        let mut word = Vec::with_capacity(ids.len());
        for &idx in ids {
            let name = letter_names.get(idx).ok_or_else(|| {
                ExperimentError::InvalidConfig(format!("letter id {idx} out of range"))
            })?;
            word.push(name.clone());
        }
        let line = TermLine {
            word,
            coeff: format_coeff(coeff),
        };
        let encoded = serde_json::to_string(&line).map_err(|err| {
            ExperimentError::InvalidConfig(format!("json encode error: {err}"))
        })?;
        writer.write_all(encoded.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

fn format_coeff(coeff: &Coeff) -> String {
    let numer = *coeff.numer();
    let denom = *coeff.denom();
    if denom == 1 {
        numer.to_string()
    } else {
        format!("{numer}/{denom}")
    }
}

fn predict_unconstrained_words(weight: usize) -> Option<u64> {
    if weight >= 63 {
        return None;
    }
    1u64.checked_shl(weight as u32)
}

fn predict_max_alternations_words(weight: usize, max_alternations: usize) -> Option<u64> {
    if weight == 0 {
        return Some(1);
    }
    let n = weight.saturating_sub(1);
    let max_k = max_alternations.min(n);
    let mut sum = 0u64;
    for k in 0..=max_k {
        let add = binom_u64(n, k)?;
        sum = sum.checked_add(add)?;
    }
    sum.checked_mul(2)
}

fn binom_u64(n: usize, k: usize) -> Option<u64> {
    if k > n {
        return Some(0);
    }
    let k = k.min(n - k);
    let mut value: u128 = 1;
    for i in 1..=k {
        let numerator = (n - k + i) as u128;
        value = value.checked_mul(numerator)?;
        value /= i as u128;
        if value > u64::MAX as u128 {
            return None;
        }
    }
    Some(value as u64)
}

fn seed_from_params(mode: Path1Mode, weight: usize, max_alternations: Option<usize>) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for &b in b"path1-toy" {
        hash = fnv1a_step(hash, b);
    }
    for &b in mode.as_str().as_bytes() {
        hash = fnv1a_step(hash, b);
    }
    for b in weight.to_le_bytes() {
        hash = fnv1a_step(hash, b);
    }
    if let Some(value) = max_alternations {
        for b in value.to_le_bytes() {
            hash = fnv1a_step(hash, b);
        }
    }
    hash
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

fn fnv1a_step(hash: u64, byte: u8) -> u64 {
    let mut out = hash ^ (byte as u64);
    out = out.wrapping_mul(FNV_PRIME);
    out
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut z = self.state.wrapping_add(0x9E3779B97F4A7C15);
        self.state = z;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

fn coeff_from_rng(rng: &mut SplitMix64) -> Coeff {
    match (rng.next_u64() & 3) as i64 {
        0 => Coeff::from_integer(-2),
        1 => Coeff::from_integer(-1),
        2 => Coeff::from_integer(1),
        _ => Coeff::from_integer(2),
    }
}

fn render_summary(cfg: &Path1ToyConfig, report: &Path1ToyReport) -> String {
    let mut out = String::new();
    out.push_str("# path1_toy\n\n");
    out.push_str(&format!("mode = {}\n\n", cfg.mode.as_str()));

    match cfg.mode {
        Path1Mode::Oracle => {
            out.push_str(&format!("weights = {:?}\n\n", report.weights));
            out.push_str("## oracle_outputs\n");
            out.push_str("- `oracle/basis_stats.csv`\n");
            out.push_str("- `oracle/oracle_checks.csv`\n");
            if cfg.export_oracle_jsonl {
                out.push_str("- `oracle/symbols_jsonl/Esymb_L*.jsonl`\n");
            }
        }
        Path1Mode::Scaled => {
            out.push_str(&format!("loops = {:?}\n\n", report.loops));
            out.push_str(&format!(
                "max_alternations = {}\n\n",
                cfg.max_alternations
            ));
            out.push_str("## scaled_outputs\n");
            out.push_str("- `scaled/basis_stats.csv`\n");
            out.push_str("- `scaled/loop_stats.csv`\n");
            out.push_str("- `scaled/esymb_jsonl/Esymb_L*.jsonl`\n");
            if report.ran_esymb {
                out.push_str("- `scaled/esymb_rank_scan/summary.md`\n");
                out.push_str("- `scaled/esymb_rank_scan/marginals_observables.csv`\n");
                out.push_str("- `scaled/esymb_rank_scan/marginals_matrix_rank.csv`\n");
                out.push_str("- `scaled/esymb_span_deps/span_deps.md`\n");
                out.push_str("- `scaled/esymb_hankel_subblock/hankel_subblock.md`\n");
            }
        }
    }

    out
}
