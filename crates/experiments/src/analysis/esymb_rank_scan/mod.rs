use std::path::{Path, PathBuf};

use mpl_symbol::Coeff;
use num_traits::Zero;

use crate::analysis::esymb_rank_scan::family::SequenceSpec;
use crate::analysis::esymb_rank_scan::io::{read_esymb_jsonl_meta, stream_esymb_terms};
use crate::analysis::esymb_rank_scan::normalize::{normalize_values, odd_double_factorial};
use crate::analysis::esymb_rank_scan::rank::{
    detect_plateau, rank_curve_float, rank_curve_mod_p, rank_curve_subsample,
};
use crate::analysis::esymb_rank_scan::report::{
    render_esymb_rank_scan_csv, render_esymb_rank_scan_md, write_esymb_rank_scan_report,
};
use crate::analysis::esymb_rank_scan::solve::{
    predict_next_value, solve_recurrence, verify_recurrence,
};
use crate::ExperimentError;

pub mod family;
pub mod io;
pub mod normalize;
pub mod rank;
pub mod report;
pub mod solve;

pub use normalize::{NormalizeChoice, NormalizeMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphabetMode {
    Manual,
    Auto,
}

impl AlphabetMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairsMode {
    Manual,
    Auto,
}

impl PairsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EsymbRankScanConfig {
    pub data_dir: Option<PathBuf>,
    pub glob: Option<String>,
    pub loops: Vec<usize>,
    pub family_pow_last: bool,
    pub family_block2: bool,
    pub x_set: Vec<String>,
    pub y_set: Vec<String>,
    pub pairs: Vec<String>,
    pub r_budget: usize,
    pub primes: Vec<i64>,
    pub float_rank: bool,
    pub float_tau: f64,
    pub subsample_rank: bool,
    pub subsample_size: usize,
    pub seed: u64,
    pub plateau_len: usize,
    pub normalize: NormalizeChoice,
    pub skip_trivial: bool,
    pub alphabet_mode: AlphabetMode,
    pub pairs_mode: PairsMode,
    pub attempt_solve_inconclusive: bool,
    pub out_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct LoopMeta {
    pub loop_index: usize,
    pub merged_terms: usize,
    pub source: PathBuf,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Recurrence {
    pub order: usize,
    pub coeffs: Vec<Coeff>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenStatus {
    Pass,
    Fail,
    Inconclusive,
    Trivial,
}

impl ScreenStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Inconclusive => "inconclusive",
            Self::Trivial => "trivial",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SequenceAnalysis {
    pub spec: SequenceSpec,
    pub values: Vec<Coeff>,
    pub normalized_values: Vec<Coeff>,
    pub normalize_mode: NormalizeMode,
    pub normalize_candidates_tried: Vec<NormalizeMode>,
    pub normalize_skipped: Vec<String>,
    pub nmax: usize,
    pub rank_mod_p: Vec<usize>,
    pub rank_float: Vec<usize>,
    pub rank_subsample: Vec<usize>,
    pub plateau_rank: Option<usize>,
    pub screen_status: ScreenStatus,
    pub recovered: bool,
    pub candidate_solve_attempted: bool,
    pub candidate_recurrence: Option<Recurrence>,
    pub candidate_predict_next_d: Option<Coeff>,
    pub candidate_predict_next_c: Option<Coeff>,
    pub recurrence: Option<Recurrence>,
    pub predict_next_d: Option<Coeff>,
    pub predict_next_c: Option<Coeff>,
}

#[derive(Clone, Debug)]
pub struct EsymbRankScanReport {
    pub loops: Vec<usize>,
    pub loop_meta: Vec<LoopMeta>,
    pub primes: Vec<i64>,
    pub seed: u64,
    pub alphabet_mode: AlphabetMode,
    pub pairs_mode: PairsMode,
    pub auto_discovered_letters: Vec<String>,
    pub auto_discovered_pairs_count: usize,
    pub attempt_solve_inconclusive: bool,
    pub sequences: Vec<SequenceAnalysis>,
}

pub fn run_esymb_rank_scan(
    cfg: &EsymbRankScanConfig,
) -> Result<EsymbRankScanReport, ExperimentError> {
    if cfg.loops.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "no loops provided".to_string(),
        ));
    }
    if !cfg.family_pow_last && !cfg.family_block2 {
        return Err(ExperimentError::InvalidConfig(
            "no families enabled".to_string(),
        ));
    }
    if cfg.family_pow_last && (cfg.x_set.is_empty() || cfg.y_set.is_empty()) {
        return Err(ExperimentError::InvalidConfig(
            "pow-last family requires --x-set and --y-set".to_string(),
        ));
    }
    if cfg.family_block2 && cfg.pairs_mode == PairsMode::Manual && cfg.pairs.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "block2 family requires --pairs".to_string(),
        ));
    }

    let loop_paths = resolve_loop_paths(cfg)?;
    let discovery = if cfg.alphabet_mode == AlphabetMode::Auto
        || (cfg.family_block2 && cfg.pairs_mode == PairsMode::Auto)
    {
        let want_pairs = cfg.family_block2 && cfg.pairs_mode == PairsMode::Auto;
        Some(discover_letters_and_pairs(
            &cfg.loops,
            &loop_paths,
            want_pairs,
        )?)
    } else {
        None
    };

    let auto_letters = discovery
        .as_ref()
        .map(|disc| disc.letters.clone())
        .unwrap_or_default();
    let auto_pairs = discovery
        .as_ref()
        .map(|disc| disc.pairs.clone())
        .unwrap_or_default();

    let mut sequences = Vec::new();
    if cfg.family_pow_last {
        sequences.extend(family::generate_pow_last(
            &cfg.x_set, &cfg.y_set, &cfg.loops,
        ));
    }
    if cfg.family_block2 {
        if cfg.pairs_mode == PairsMode::Auto {
            if auto_pairs.is_empty() {
                return Err(ExperimentError::InvalidConfig(
                    "auto pair discovery found no block2 pairs".to_string(),
                ));
            }
            sequences.extend(family::generate_block2_pairs(&auto_pairs, &cfg.loops));
        } else {
            sequences.extend(family::generate_block2(&cfg.pairs, &cfg.loops));
        }
    }

    sequences.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    let mut values = vec![vec![Coeff::zero(); cfg.loops.len()]; sequences.len()];
    let mut loop_meta = Vec::new();
    for (loop_idx, loop_value) in cfg.loops.iter().enumerate() {
        let path = loop_paths.get(loop_value).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!("missing loop file for L={loop_value}"))
        })?;
        let meta = read_esymb_jsonl_meta(path)?;
        if meta.loop_index != *loop_value {
            return Err(ExperimentError::InvalidConfig(format!(
                "loop index mismatch for {}: expected L={}, found L={}",
                path.display(),
                loop_value,
                meta.loop_index
            )));
        }
        loop_meta.push(LoopMeta {
            loop_index: meta.loop_index,
            merged_terms: meta.merged_terms,
            source: path.clone(),
            name: meta.name.clone(),
        });

        let mut lookup = std::collections::BTreeMap::<Vec<String>, Vec<usize>>::new();
        for (seq_idx, seq) in sequences.iter().enumerate() {
            if let Some(word) = seq.words.get(loop_idx) {
                lookup.entry(word.clone()).or_default().push(seq_idx);
            }
        }

        let mut reader = stream_esymb_terms(path)?;
        while let Some(term) = reader.next_term()? {
            if let Some(indices) = lookup.get(&term.word) {
                for &seq_idx in indices {
                    values[seq_idx][loop_idx] += term.coeff;
                }
            }
        }
    }

    let mut analyses = Vec::with_capacity(sequences.len());
    for (seq_idx, spec) in sequences.into_iter().enumerate() {
        let raw_values = values[seq_idx].clone();
        let analysis = analyze_sequence(&spec, &raw_values, cfg)?;
        analyses.push(analysis);
    }

    let report = EsymbRankScanReport {
        loops: cfg.loops.clone(),
        loop_meta,
        primes: cfg.primes.clone(),
        seed: cfg.seed,
        alphabet_mode: cfg.alphabet_mode,
        pairs_mode: cfg.pairs_mode,
        auto_discovered_letters: auto_letters,
        auto_discovered_pairs_count: auto_pairs.len(),
        attempt_solve_inconclusive: cfg.attempt_solve_inconclusive,
        sequences: analyses,
    };

    write_esymb_rank_scan_report(&report, &cfg.out_dir)?;
    Ok(report)
}

#[derive(Clone, Debug)]
struct DiscoveryResult {
    letters: Vec<String>,
    pairs: Vec<(String, String)>,
}

fn discover_letters_and_pairs(
    loops: &[usize],
    loop_paths: &std::collections::BTreeMap<usize, PathBuf>,
    want_pairs: bool,
) -> Result<DiscoveryResult, ExperimentError> {
    let mut letters = std::collections::BTreeSet::new();
    let mut pairs = std::collections::BTreeSet::new();
    for &loop_value in loops {
        let path = loop_paths.get(&loop_value).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!("missing loop file for L={loop_value}"))
        })?;
        let mut reader = stream_esymb_terms(path)?;
        while let Some(term) = reader.next_term()? {
            for token in &term.word {
                letters.insert(token.clone());
            }
            if want_pairs && term.word.len() == 2 * loop_value {
                if let Some(pair) = repeated_pair(&term.word) {
                    pairs.insert(pair);
                }
            }
        }
    }
    Ok(DiscoveryResult {
        letters: letters.into_iter().collect(),
        pairs: pairs.into_iter().collect(),
    })
}

fn repeated_pair(word: &[String]) -> Option<(String, String)> {
    if word.len() < 2 || word.len() % 2 != 0 {
        return None;
    }
    let first = word.get(0)?.clone();
    let second = word.get(1)?.clone();
    for idx in (0..word.len()).step_by(2) {
        if word.get(idx)? != &first || word.get(idx + 1)? != &second {
            return None;
        }
    }
    Some((first, second))
}

#[derive(Clone, Debug)]
struct CandidateResult {
    mode: NormalizeMode,
    normalized_values: Vec<Coeff>,
    nmax: usize,
    rank_mod_p: Vec<usize>,
    rank_float: Vec<usize>,
    rank_subsample: Vec<usize>,
    plateau_rank: Option<usize>,
    screen_status: ScreenStatus,
    last_rank: usize,
    plateau_tail_len: usize,
}

#[derive(Clone, Debug)]
struct NormalizeSummary {
    tried: Vec<NormalizeMode>,
    skipped: Vec<String>,
}

fn analyze_sequence(
    spec: &SequenceSpec,
    raw_values: &[Coeff],
    cfg: &EsymbRankScanConfig,
) -> Result<SequenceAnalysis, ExperimentError> {
    let (candidates, normalize_summary) = build_candidates(spec, raw_values, cfg)?;
    let chosen = choose_candidate(candidates);

    let mut recurrence = None;
    let mut predict_next_d = None;
    let allow_solve = match chosen.screen_status {
        ScreenStatus::Pass => true,
        ScreenStatus::Trivial => !cfg.skip_trivial,
        _ => false,
    };
    let mut screen_status = chosen.screen_status;
    let mut candidate_solve_attempted = false;
    let mut candidate_recurrence = None;
    let mut candidate_predict_next_d = None;
    let mut candidate_predict_next_c = None;
    if allow_solve {
        if let Some(rank) = chosen.plateau_rank {
            if rank > 0 && rank <= cfg.r_budget {
                if let Some(candidate) = solve_recurrence(&chosen.normalized_values, rank, 0) {
                    if let Some(candidate2) = solve_recurrence(&chosen.normalized_values, rank, 1) {
                        if solve::equivalent_recurrence(&candidate, &candidate2) {
                            if verify_recurrence(&chosen.normalized_values, &candidate) {
                                predict_next_d =
                                    predict_next_value(&chosen.normalized_values, &candidate);
                                recurrence = Some(Recurrence {
                                    order: candidate.order,
                                    coeffs: candidate.coeffs,
                                });
                            }
                        }
                    }
                }
            }
        }
    } else if cfg.attempt_solve_inconclusive && screen_status == ScreenStatus::Inconclusive {
        let order = chosen.nmax.saturating_add(1);
        if order <= cfg.r_budget {
            candidate_solve_attempted = true;
            if let Some(candidate) = solve_recurrence(&chosen.normalized_values, order, 0) {
                if verify_recurrence(&chosen.normalized_values, &candidate) {
                    candidate_predict_next_d =
                        predict_next_value(&chosen.normalized_values, &candidate);
                    candidate_recurrence = Some(Recurrence {
                        order: candidate.order,
                        coeffs: candidate.coeffs,
                    });
                    candidate_predict_next_c = candidate_predict_next_d
                        .as_ref()
                        .and_then(|value| map_predict_next_c(value, chosen.mode, &cfg.loops));
                    let candidate_pass = candidate_recurrence
                        .as_ref()
                        .map(|rec| candidate_filters_pass(rec, &candidate_predict_next_c))
                        .unwrap_or(false);
                    if candidate_pass {
                        screen_status = ScreenStatus::Pass;
                        recurrence = candidate_recurrence.clone();
                        predict_next_d = candidate_predict_next_d.clone();
                    }
                }
            }
        }
    }

    let predict_next_c = predict_next_d
        .as_ref()
        .and_then(|value| map_predict_next_c(value, chosen.mode, &cfg.loops));
    let recovered =
        screen_status == ScreenStatus::Pass && raw_values.iter().any(|value| !value.is_zero());

    Ok(SequenceAnalysis {
        spec: spec.clone(),
        values: raw_values.to_vec(),
        normalized_values: chosen.normalized_values,
        normalize_mode: chosen.mode,
        normalize_candidates_tried: normalize_summary.tried,
        normalize_skipped: normalize_summary.skipped,
        nmax: chosen.nmax,
        rank_mod_p: chosen.rank_mod_p,
        rank_float: chosen.rank_float,
        rank_subsample: chosen.rank_subsample,
        plateau_rank: chosen.plateau_rank,
        screen_status,
        recovered,
        candidate_solve_attempted,
        candidate_recurrence,
        candidate_predict_next_d,
        candidate_predict_next_c,
        recurrence,
        predict_next_d,
        predict_next_c,
    })
}

fn build_candidates(
    spec: &SequenceSpec,
    raw_values: &[Coeff],
    cfg: &EsymbRankScanConfig,
) -> Result<(Vec<CandidateResult>, NormalizeSummary), ExperimentError> {
    let mut out = Vec::new();
    let mut tried = Vec::new();
    let mut skipped = Vec::new();
    let modes: Vec<NormalizeMode> = match cfg.normalize {
        NormalizeChoice::None => vec![NormalizeMode::None],
        NormalizeChoice::OddDoubleFactorial => vec![NormalizeMode::OddDoubleFactorial],
        NormalizeChoice::EvenDoubleFactorial => vec![NormalizeMode::EvenDoubleFactorial],
        NormalizeChoice::FactorialLm1 => vec![NormalizeMode::FactorialLm1],
        NormalizeChoice::CentralBinomialLm1 => vec![NormalizeMode::CentralBinomialLm1],
        NormalizeChoice::Auto => match spec.family {
            family::FamilyType::PowLast => {
                vec![NormalizeMode::None, NormalizeMode::OddDoubleFactorial]
            }
            family::FamilyType::Block2 => vec![
                NormalizeMode::None,
                NormalizeMode::OddDoubleFactorial,
                NormalizeMode::EvenDoubleFactorial,
                NormalizeMode::FactorialLm1,
                NormalizeMode::CentralBinomialLm1,
            ],
        },
    };

    for mode in modes {
        tried.push(mode);
        let normalized_values = match normalize_values(raw_values, &cfg.loops, mode) {
            Ok(values) => values,
            Err(err) => {
                skipped.push(format!("{}:{}", mode.as_str(), err));
                continue;
            }
        };
        let nmax = rank::compute_nmax(normalized_values.len());
        let rank_mod_p = rank_curve_mod_p(&normalized_values, &cfg.primes, nmax)?;
        let rank_float = if cfg.float_rank {
            rank_curve_float(&normalized_values, nmax, cfg.float_tau)
        } else {
            Vec::new()
        };
        let rank_subsample = if cfg.subsample_rank {
            rank_curve_subsample(
                &normalized_values,
                &cfg.primes,
                nmax,
                cfg.subsample_size,
                cfg.seed,
            )?
        } else {
            Vec::new()
        };
        let plateau_rank = detect_plateau(&rank_mod_p, cfg.plateau_len);
        let screen_status = classify_screen(
            &normalized_values,
            &rank_mod_p,
            plateau_rank,
            nmax,
            cfg.r_budget,
        );
        let last_rank = rank_mod_p.last().copied().unwrap_or(0);
        let plateau_tail_len = plateau_tail_len(&rank_mod_p);
        out.push(CandidateResult {
            mode,
            normalized_values,
            nmax,
            rank_mod_p,
            rank_float,
            rank_subsample,
            plateau_rank,
            screen_status,
            last_rank,
            plateau_tail_len,
        });
    }
    if out.is_empty() {
        let fallback_values = raw_values.to_vec();
        let nmax = rank::compute_nmax(fallback_values.len());
        let rank_mod_p = rank_curve_mod_p(&fallback_values, &cfg.primes, nmax)?;
        let rank_float = if cfg.float_rank {
            rank_curve_float(&fallback_values, nmax, cfg.float_tau)
        } else {
            Vec::new()
        };
        let rank_subsample = if cfg.subsample_rank {
            rank_curve_subsample(
                &fallback_values,
                &cfg.primes,
                nmax,
                cfg.subsample_size,
                cfg.seed,
            )?
        } else {
            Vec::new()
        };
        let plateau_rank = detect_plateau(&rank_mod_p, cfg.plateau_len);
        let screen_status = classify_screen(
            &fallback_values,
            &rank_mod_p,
            plateau_rank,
            nmax,
            cfg.r_budget,
        );
        let last_rank = rank_mod_p.last().copied().unwrap_or(0);
        let plateau_tail_len = plateau_tail_len(&rank_mod_p);
        out.push(CandidateResult {
            mode: NormalizeMode::None,
            normalized_values: fallback_values,
            nmax,
            rank_mod_p,
            rank_float,
            rank_subsample,
            plateau_rank,
            screen_status,
            last_rank,
            plateau_tail_len,
        });
    }
    Ok((out, NormalizeSummary { tried, skipped }))
}

fn choose_candidate(mut candidates: Vec<CandidateResult>) -> CandidateResult {
    candidates.sort_by(|a, b| compare_candidates(a, b));
    candidates.swap_remove(0)
}

fn compare_candidates(a: &CandidateResult, b: &CandidateResult) -> std::cmp::Ordering {
    let a_priority = status_priority(a.screen_status);
    let b_priority = status_priority(b.screen_status);
    if a_priority != b_priority {
        return a_priority.cmp(&b_priority);
    }
    let a_rank = a.plateau_rank.unwrap_or(a.last_rank);
    let b_rank = b.plateau_rank.unwrap_or(b.last_rank);
    if a_rank != b_rank {
        return a_rank.cmp(&b_rank);
    }
    if a.plateau_tail_len != b.plateau_tail_len {
        return b.plateau_tail_len.cmp(&a.plateau_tail_len);
    }
    a.mode.order_key().cmp(&b.mode.order_key())
}

fn status_priority(status: ScreenStatus) -> usize {
    match status {
        ScreenStatus::Pass => 0,
        ScreenStatus::Inconclusive => 1,
        ScreenStatus::Fail => 2,
        ScreenStatus::Trivial => 3,
    }
}

fn plateau_tail_len(curve: &[usize]) -> usize {
    if curve.is_empty() {
        return 0;
    }
    let last = curve[curve.len() - 1];
    let mut count = 1usize;
    for value in curve[..curve.len() - 1].iter().rev() {
        if *value == last {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn classify_screen(
    values: &[Coeff],
    rank_mod_p: &[usize],
    plateau_rank: Option<usize>,
    nmax: usize,
    r_budget: usize,
) -> ScreenStatus {
    if values.iter().all(|value| value.is_zero()) {
        return ScreenStatus::Trivial;
    }
    if let Some(rank) = plateau_rank {
        if rank <= r_budget {
            return ScreenStatus::Pass;
        }
        return ScreenStatus::Fail;
    }
    let last_rank = rank_mod_p.last().copied().unwrap_or(0);
    if last_rank > r_budget {
        return ScreenStatus::Fail;
    }
    if nmax < r_budget {
        return ScreenStatus::Inconclusive;
    }
    ScreenStatus::Inconclusive
}

fn map_predict_next_c(
    predict_next_d: &Coeff,
    mode: NormalizeMode,
    loops: &[usize],
) -> Option<Coeff> {
    let last_loop = loops.iter().copied().max()?;
    let next_loop = last_loop.saturating_add(1);
    match mode {
        NormalizeMode::None => Some(*predict_next_d),
        NormalizeMode::OddDoubleFactorial => {
            let factor = odd_double_factorial(next_loop).ok()?;
            Some(*predict_next_d * factor)
        }
        NormalizeMode::EvenDoubleFactorial => {
            let factor = normalize::even_double_factorial(next_loop).ok()?;
            Some(*predict_next_d * factor)
        }
        NormalizeMode::FactorialLm1 => {
            let factor = normalize::factorial_lm1(next_loop).ok()?;
            Some(*predict_next_d * factor)
        }
        NormalizeMode::CentralBinomialLm1 => {
            let factor = normalize::central_binomial_lm1(next_loop).ok()?;
            Some(*predict_next_d * factor)
        }
    }
}

fn candidate_filters_pass(rec: &Recurrence, predict_next_c: &Option<Coeff>) -> bool {
    const COEFF_BOUND: i64 = 1_000_000;
    if rec
        .coeffs
        .iter()
        .any(|coeff| !coeff_within_bound(coeff, COEFF_BOUND))
    {
        return false;
    }
    match predict_next_c.as_ref() {
        Some(value) => *value.denom() == 1,
        None => false,
    }
}

fn coeff_within_bound(coeff: &Coeff, bound: i64) -> bool {
    let numer = coeff.numer().abs();
    let denom = coeff.denom().abs();
    numer <= bound && denom <= bound
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        path.push(format!("mpl_esymb_{name}_{stamp}.jsonl"));
        path
    }

    #[test]
    fn auto_pairs_detects_repeated_block() {
        let path_l1 = temp_path("auto_pairs_l1");
        let path_l2 = temp_path("auto_pairs_l2");
        let content_l1 = r#"{"_meta":{"name":"Esymb","loop":1,"merged_terms":2}}
{"word":["a","b"],"coeff":"1"}
{"word":["a","c"],"coeff":"1"}
"#;
        let content_l2 = r#"{"_meta":{"name":"Esymb","loop":2,"merged_terms":2}}
{"word":["d","e","d","e"],"coeff":"1"}
{"word":["d","e","e","d"],"coeff":"1"}
"#;
        fs::write(&path_l1, content_l1).expect("write l1");
        fs::write(&path_l2, content_l2).expect("write l2");

        let mut loop_paths = std::collections::BTreeMap::new();
        loop_paths.insert(1, path_l1.clone());
        loop_paths.insert(2, path_l2.clone());
        let discovery = discover_letters_and_pairs(&[1, 2], &loop_paths, true).expect("discovery");
        assert_eq!(
            discovery.letters,
            vec!["a", "b", "c", "d", "e"]
                .into_iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            discovery.pairs,
            vec![
                ("a".to_string(), "b".to_string()),
                ("a".to_string(), "c".to_string()),
                ("d".to_string(), "e".to_string())
            ]
        );

        let _ = fs::remove_file(&path_l1);
        let _ = fs::remove_file(&path_l2);
    }

    #[test]
    fn normalize_tiebreak_order_prefers_none() {
        let mut candidates = Vec::new();
        for mode in [
            NormalizeMode::OddDoubleFactorial,
            NormalizeMode::None,
            NormalizeMode::EvenDoubleFactorial,
        ] {
            candidates.push(CandidateResult {
                mode,
                normalized_values: vec![Coeff::from_integer(1)],
                nmax: 0,
                rank_mod_p: vec![1],
                rank_float: Vec::new(),
                rank_subsample: Vec::new(),
                plateau_rank: Some(1),
                screen_status: ScreenStatus::Inconclusive,
                last_rank: 1,
                plateau_tail_len: 1,
            });
        }
        let chosen = choose_candidate(candidates);
        assert_eq!(chosen.mode, NormalizeMode::None);
    }
}

fn resolve_loop_paths(
    cfg: &EsymbRankScanConfig,
) -> Result<std::collections::BTreeMap<usize, PathBuf>, ExperimentError> {
    let mut map = std::collections::BTreeMap::new();
    if let Some(pattern) = cfg.glob.as_ref() {
        let paths = glob::glob(pattern)
            .map_err(|err| ExperimentError::InvalidConfig(format!("invalid glob: {err}")))?;
        for entry in paths {
            let path = entry
                .map_err(|err| ExperimentError::InvalidConfig(format!("glob error: {err}")))?;
            if let Some(loop_index) = parse_loop_index(&path) {
                map.insert(loop_index, path);
            }
        }
    }
    if let Some(dir) = cfg.data_dir.as_ref() {
        for &loop_index in &cfg.loops {
            let filename = format!("Esymb_L{loop_index}.jsonl");
            let path = dir.join(filename);
            map.insert(loop_index, path);
        }
    }
    Ok(map)
}

fn parse_loop_index(path: &Path) -> Option<usize> {
    let name = path.file_name()?.to_string_lossy();
    let lower = name.to_ascii_lowercase();
    let start = lower.find("_l")?;
    let digits = lower[start + 2..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    digits.parse::<usize>().ok()
}

pub fn render_esymb_rank_scan_outputs(report: &EsymbRankScanReport) -> (String, String) {
    let csv = render_esymb_rank_scan_csv(report);
    let md = render_esymb_rank_scan_md(report);
    (csv, md)
}
