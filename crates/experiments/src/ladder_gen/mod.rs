use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use mpl_ir::Expr;
use mpl_symbol::space::check_integrable_n;
use mpl_symbol::{Coeff, ShuffleFuel, Symbol, Word};
use num_traits::{One, Zero};
use serde::Serialize;

use crate::analysis::esymb_rank_scan::family::{
    format_letters_compact, FamilyType, SequenceSource, SequenceSpec,
};
use crate::analysis::esymb_rank_scan::observables::{
    render_marginals_matrix_rank_csv, render_marginals_observables_csv, MatrixRankRow,
};
use crate::analysis::esymb_rank_scan::rank::rank_matrix_mod_p;
use crate::ExperimentError;

const LETTER_NAMES: [&str; 4] = ["z", "zbar", "1-z", "1-zbar"];
const ID_Z: usize = 0;
const ID_ZBAR: usize = 1;
const ID_ONE_MINUS_Z: usize = 2;
const ID_ONE_MINUS_ZBAR: usize = 3;
const DEFAULT_MAX_TERMS: u64 = 5_000_000;
const DEFAULT_REF_LOOP: usize = 5;
const DEFAULT_PRIMES: [i64; 3] = [1000003, 1000033, 1000037];
type LadderMarginals = (Vec<SequenceSpec>, Vec<Vec<Coeff>>);
type LadderPairMarginals = (Vec<SequenceSpec>, Vec<Vec<Coeff>>, PairValues);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LadderFamily {
    Prefix,
    Suffix,
    PrefixSuffix,
}

impl LadderFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prefix => "prefix",
            Self::Suffix => "suffix",
            Self::PrefixSuffix => "prefix-suffix",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "prefix" => Some(Self::Prefix),
            "suffix" => Some(Self::Suffix),
            "prefix-suffix" => Some(Self::PrefixSuffix),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LadderGenConfig {
    pub out_dir: PathBuf,
    pub loops: Vec<usize>,
    pub prefix_len: usize,
    pub suffix_len: usize,
    pub family: LadderFamily,
    pub emit_jsonl: bool,
    pub max_terms: u64,
    pub data_dir: Option<PathBuf>,
    pub validate: bool,
    pub matrix_rank: bool,
    pub reference_bruteforce_max_loop: usize,
}

impl Default for LadderGenConfig {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::new(),
            loops: Vec::new(),
            prefix_len: 2,
            suffix_len: 2,
            family: LadderFamily::PrefixSuffix,
            emit_jsonl: false,
            max_terms: DEFAULT_MAX_TERMS,
            data_dir: None,
            validate: true,
            matrix_rank: false,
            reference_bruteforce_max_loop: DEFAULT_REF_LOOP,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LadderGenReport {
    pub out_dir: PathBuf,
    pub loops: Vec<usize>,
    pub data_dir: Option<PathBuf>,
    pub wrote_jsonl: bool,
}

/// Drummond et al. 2010 ladders: eqs. 3.29-3.31 (definitions), eq. 3.32 (closed form),
/// eq. 3.34 (loop-lowering differential equation).
pub fn run_ladder_gen(cfg: &LadderGenConfig) -> Result<LadderGenReport, ExperimentError> {
    if cfg.out_dir.as_os_str().is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "missing out_dir".to_string(),
        ));
    }
    if cfg.loops.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "loops list is empty".to_string(),
        ));
    }
    if cfg.emit_jsonl && cfg.max_terms == 0 {
        return Err(ExperimentError::InvalidConfig(
            "max_terms must be >= 1 when emitting jsonl".to_string(),
        ));
    }
    if cfg.matrix_rank && cfg.family != LadderFamily::PrefixSuffix {
        return Err(ExperimentError::InvalidConfig(
            "matrix_rank requires prefix-suffix family".to_string(),
        ));
    }

    let mut loops = cfg.loops.clone();
    loops.sort_unstable();
    loops.dedup();
    if loops.contains(&0) {
        return Err(ExperimentError::InvalidConfig(
            "loops must be >= 1".to_string(),
        ));
    }

    let min_loop = loops[0];
    let min_len = min_loop
        .checked_mul(2)
        .ok_or_else(|| ExperimentError::InvalidConfig("loop * 2 overflow".to_string()))?;
    match cfg.family {
        LadderFamily::Prefix => {
            if cfg.prefix_len > min_len {
                return Err(ExperimentError::InvalidConfig(format!(
                    "prefix length {} exceeds min word length {min_len}",
                    cfg.prefix_len
                )));
            }
        }
        LadderFamily::Suffix => {
            if cfg.suffix_len > min_len {
                return Err(ExperimentError::InvalidConfig(format!(
                    "suffix length {} exceeds min word length {min_len}",
                    cfg.suffix_len
                )));
            }
        }
        LadderFamily::PrefixSuffix => {
            if cfg.prefix_len.saturating_add(cfg.suffix_len) > min_len {
                return Err(ExperimentError::InvalidConfig(format!(
                    "prefix-suffix lengths r={},k={} exceed min word length {min_len}",
                    cfg.prefix_len, cfg.suffix_len
                )));
            }
        }
    }

    fs::create_dir_all(&cfg.out_dir)?;

    let data_dir = if cfg.emit_jsonl {
        let dir = cfg
            .data_dir
            .clone()
            .unwrap_or_else(|| cfg.out_dir.join("converted_jsonl"));
        fs::create_dir_all(&dir)?;
        Some(dir)
    } else {
        None
    };

    let (sequences, values, pair_values) = match cfg.family {
        LadderFamily::Prefix => {
            let (seq, vals) = compute_prefix_marginals(&loops, cfg.prefix_len)?;
            (seq, vals, None)
        }
        LadderFamily::Suffix => {
            let (seq, vals) = compute_suffix_marginals(&loops, cfg.suffix_len)?;
            (seq, vals, None)
        }
        LadderFamily::PrefixSuffix => {
            let (seq, vals, pair) =
                compute_prefix_suffix_marginals(&loops, cfg.prefix_len, cfg.suffix_len)?;
            (seq, vals, Some(pair))
        }
    };

    fs::write(
        cfg.out_dir.join("marginals_observables.csv"),
        render_marginals_observables_csv(&sequences, &values, &loops),
    )?;

    if cfg.matrix_rank {
        if let Some(pair_values) = pair_values.as_ref() {
            let rows =
                compute_matrix_rank_rows(&loops, cfg.prefix_len, cfg.suffix_len, pair_values)?;
            fs::write(
                cfg.out_dir.join("marginals_matrix_rank.csv"),
                render_marginals_matrix_rank_csv(&rows),
            )?;
        }
    }

    if let Some(dir) = data_dir.as_ref() {
        for &loop_value in &loops {
            let path = dir.join(format!("Esymb_L{loop_value}.jsonl"));
            write_ladder_jsonl(&path, loop_value, cfg.max_terms)?;
        }
    }

    if cfg.validate {
        let max_loop = loops.iter().copied().max().unwrap_or(0);
        let limit = cfg.reference_bruteforce_max_loop.min(max_loop);
        if limit > 0 {
            validate_loop_lowering(limit)?;
        }
    }

    Ok(LadderGenReport {
        out_dir: cfg.out_dir.clone(),
        loops,
        data_dir,
        wrote_jsonl: cfg.emit_jsonl,
    })
}

pub fn ladder_symbol_combinatorial(loop_value: usize) -> Result<Symbol, ExperimentError> {
    if loop_value == 0 {
        return Ok(Symbol::zero());
    }
    let exprs = ladder_letter_exprs();
    let mut terms: Vec<(Word, Coeff)> = Vec::new();
    for_each_ladder_term(loop_value, |ids, coeff| {
        let letters = ids
            .iter()
            .map(|&id| {
                exprs.get(id).cloned().ok_or_else(|| {
                    ExperimentError::InvalidConfig(format!("letter id {id} out of range"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        terms.push((Word(letters), coeff));
        Ok(())
    })?;
    Ok(Symbol::from_terms(terms))
}

/// Brute-force symbol from eq. 3.32 using shuffle algebra.
pub fn ladder_symbol_bruteforce(loop_value: usize) -> Result<Symbol, ExperimentError> {
    if loop_value == 0 {
        return Ok(Symbol::zero());
    }
    let exprs = ladder_letter_exprs();
    let z = exprs[ID_Z].clone();
    let zbar = exprs[ID_ZBAR].clone();

    let log_sym = Symbol::from_terms(vec![
        (Word(vec![z.clone()]), Coeff::one()),
        (Word(vec![zbar.clone()]), Coeff::one()),
    ]);

    let mut total = Symbol::zero();
    for r in 0..=loop_value {
        let n = 2 * loop_value - r;
        let li_z = li_symbol(n, true, &exprs);
        let li_zbar = li_symbol(n, false, &exprs);
        let li_diff = symbol_sub(&li_z, &li_zbar);

        let log_pow = if r == 0 {
            symbol_one()
        } else {
            let mut fuel = ShuffleFuel::unlimited();
            log_sym.shuffle_pow(r as u32, &mut fuel)?
        };
        let mut fuel = ShuffleFuel::unlimited();
        let product = log_pow.shuffle_mul(&li_diff, &mut fuel)?;
        let coeff = ladder_coeff(loop_value, r)?;
        total = symbol_add(&total, &symbol_scale(&product, coeff));
    }
    Ok(total)
}

/// Apply z*d/dz and zbar*d/dzbar in both orders (eq. 3.34).
pub fn ladder_de_down(sym: &Symbol) -> (Symbol, Symbol) {
    let exprs = ladder_letter_exprs();
    let dz = strip_last(sym, &exprs[ID_Z]);
    let dzbar = strip_last(sym, &exprs[ID_ZBAR]);
    let down_zbar_z = strip_last(&dzbar, &exprs[ID_Z]);
    let down_z_zbar = strip_last(&dz, &exprs[ID_ZBAR]);
    (down_zbar_z, down_z_zbar)
}

fn validate_loop_lowering(max_loop: usize) -> Result<(), ExperimentError> {
    let mut symbols = Vec::with_capacity(max_loop);
    for loop_value in 1..=max_loop {
        let sym = ladder_symbol_combinatorial(loop_value)?;
        if !check_integrable_n(&sym)? {
            return Err(ExperimentError::InvalidConfig(format!(
                "ladder symbol not integrable at L={loop_value}"
            )));
        }
        symbols.push(sym);
    }
    if max_loop >= 2 {
        for loop_value in 2..=max_loop {
            let sym = &symbols[loop_value - 1];
            let prev = &symbols[loop_value - 2];
            let (down1, down2) = ladder_de_down(sym);
            if &down1 != prev {
                return Err(ExperimentError::InvalidConfig(format!(
                    "loop-lowering mismatch at L={loop_value} (zbar then z)"
                )));
            }
            if &down2 != prev {
                return Err(ExperimentError::InvalidConfig(format!(
                    "loop-lowering mismatch at L={loop_value} (z then zbar)"
                )));
            }
        }
    }
    Ok(())
}

fn compute_prefix_marginals(
    loops: &[usize],
    prefix_len: usize,
) -> Result<LadderMarginals, ExperimentError> {
    let prefix_keys = enumerate_words(LETTER_NAMES.len(), prefix_len);
    let mut sequences = Vec::with_capacity(prefix_keys.len());
    let mut values = Vec::with_capacity(prefix_keys.len());
    for ids in prefix_keys {
        let names = ids_to_names(&ids);
        let params = vec![
            format!("r={prefix_len}"),
            format!("p={}", format_letters_compact(&names)),
        ];
        sequences.push(SequenceSpec {
            family: FamilyType::Prefix,
            params,
            source: SequenceSource::Prefix {
                prefix: names.clone(),
            },
        });
        let mut row = Vec::with_capacity(loops.len());
        for &loop_value in loops {
            row.push(ladder_marginal_count(loop_value, &ids, &[])?);
        }
        values.push(row);
    }
    Ok((sequences, values))
}

fn compute_suffix_marginals(
    loops: &[usize],
    suffix_len: usize,
) -> Result<LadderMarginals, ExperimentError> {
    let suffix_keys = enumerate_words(LETTER_NAMES.len(), suffix_len);
    let mut sequences = Vec::with_capacity(suffix_keys.len());
    let mut values = Vec::with_capacity(suffix_keys.len());
    for ids in suffix_keys {
        let names = ids_to_names(&ids);
        let params = vec![
            format!("k={suffix_len}"),
            format!("s={}", format_letters_compact(&names)),
        ];
        sequences.push(SequenceSpec {
            family: FamilyType::Suffix,
            params,
            source: SequenceSource::Suffix {
                suffix: names.clone(),
            },
        });
        let mut row = Vec::with_capacity(loops.len());
        for &loop_value in loops {
            row.push(ladder_marginal_count(loop_value, &[], &ids)?);
        }
        values.push(row);
    }
    Ok((sequences, values))
}

fn compute_prefix_suffix_marginals(
    loops: &[usize],
    prefix_len: usize,
    suffix_len: usize,
) -> Result<LadderPairMarginals, ExperimentError> {
    let prefix_keys = enumerate_words(LETTER_NAMES.len(), prefix_len);
    let suffix_keys = enumerate_words(LETTER_NAMES.len(), suffix_len);
    let mut pair_values = PairValues::new(prefix_keys.len(), suffix_keys.len(), loops.len());
    for (i, prefix_ids) in prefix_keys.iter().enumerate() {
        for (j, suffix_ids) in suffix_keys.iter().enumerate() {
            for (loop_idx, &loop_value) in loops.iter().enumerate() {
                pair_values.values[i][j][loop_idx] =
                    ladder_marginal_count(loop_value, prefix_ids, suffix_ids)?;
            }
        }
    }

    let mut sequences = Vec::with_capacity(prefix_keys.len() * suffix_keys.len());
    let mut values = Vec::with_capacity(prefix_keys.len() * suffix_keys.len());
    for (i, prefix_ids) in prefix_keys.iter().enumerate() {
        let prefix_names = ids_to_names(prefix_ids);
        for (j, suffix_ids) in suffix_keys.iter().enumerate() {
            let suffix_names = ids_to_names(suffix_ids);
            let params = vec![
                format!("r={prefix_len}"),
                format!("k={suffix_len}"),
                format!("u={}", format_letters_compact(&prefix_names)),
                format!("v={}", format_letters_compact(&suffix_names)),
            ];
            sequences.push(SequenceSpec {
                family: FamilyType::PrefixSuffix,
                params,
                source: SequenceSource::PrefixSuffix {
                    prefix: prefix_names.clone(),
                    suffix: suffix_names.clone(),
                },
            });
            values.push(pair_values.values[i][j].clone());
        }
    }
    Ok((sequences, values, pair_values))
}

fn compute_matrix_rank_rows(
    loops: &[usize],
    prefix_len: usize,
    suffix_len: usize,
    pair_values: &PairValues,
) -> Result<Vec<MatrixRankRow>, ExperimentError> {
    let nrows = pair_values.nrows();
    let ncols = pair_values.ncols();
    let mut rows = Vec::with_capacity(loops.len());
    for (loop_idx, &loop_value) in loops.iter().enumerate() {
        if nrows == 0 || ncols == 0 {
            rows.push(MatrixRankRow {
                loop_index: loop_value,
                prefix_len,
                suffix_len,
                nrows,
                ncols,
                rank_mod_p: 0,
            });
            continue;
        }
        let mut matrix = vec![vec![Coeff::zero(); ncols]; nrows];
        for (i, row) in matrix.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = pair_values.values[i][j][loop_idx];
            }
        }
        let rank_mod_p = rank_matrix_mod_p(&matrix, &DEFAULT_PRIMES)?;
        rows.push(MatrixRankRow {
            loop_index: loop_value,
            prefix_len,
            suffix_len,
            nrows,
            ncols,
            rank_mod_p,
        });
    }
    Ok(rows)
}

struct PairValues {
    values: Vec<Vec<Vec<Coeff>>>,
}

impl PairValues {
    fn new(nrows: usize, ncols: usize, loops: usize) -> Self {
        Self {
            values: vec![vec![vec![Coeff::zero(); loops]; ncols]; nrows],
        }
    }

    fn nrows(&self) -> usize {
        self.values.len()
    }

    fn ncols(&self) -> usize {
        self.values.first().map(|row| row.len()).unwrap_or(0)
    }
}

pub fn ladder_marginal_count(
    loop_value: usize,
    prefix: &[usize],
    suffix: &[usize],
) -> Result<Coeff, ExperimentError> {
    if loop_value == 0 {
        return Err(ExperimentError::InvalidConfig(
            "loop must be >= 1".to_string(),
        ));
    }
    let count = ladder_marginal_count_i128(loop_value, prefix, suffix)?;
    coeff_from_i128(count)
}

fn ladder_marginal_count_i128(
    loop_value: usize,
    prefix: &[usize],
    suffix: &[usize],
) -> Result<i128, ExperimentError> {
    let n = loop_value
        .checked_mul(2)
        .ok_or_else(|| ExperimentError::InvalidConfig("loop * 2 overflow".to_string()))?;
    if prefix.len() + suffix.len() > n {
        return Err(ExperimentError::InvalidConfig(format!(
            "prefix+suffix length exceeds word length: {}+{} > {n}",
            prefix.len(),
            suffix.len()
        )));
    }
    for &id in prefix.iter().chain(suffix.iter()) {
        if id >= LETTER_NAMES.len() {
            return Err(ExperimentError::InvalidConfig(format!(
                "letter id {id} out of range"
            )));
        }
    }

    let mut fixed = vec![None; n];
    for (idx, &id) in prefix.iter().enumerate() {
        match fixed[idx] {
            Some(prev) if prev != id => return Ok(0),
            Some(_) => {}
            None => fixed[idx] = Some(id),
        }
    }
    for (offset, &id) in suffix.iter().enumerate() {
        let pos = n - suffix.len() + offset;
        match fixed[pos] {
            Some(prev) if prev != id => return Ok(0),
            Some(_) => {}
            None => fixed[pos] = Some(id),
        }
    }

    let mut total = 0i128;
    for &special_id in &[ID_ONE_MINUS_Z, ID_ONE_MINUS_ZBAR] {
        for p in 0..=loop_value {
            if p >= n {
                continue;
            }
            if let Some(id) = fixed[p] {
                if id != special_id {
                    continue;
                }
            }
            if fixed_special_conflict(&fixed, p) {
                continue;
            }

            let m = n - p - 1;
            let (fixed_z, _fixed_zbar, free_prefix, free_suffix) = fixed_counts(&fixed, p, n)?;

            let ways_prefix = pow2_i128(free_prefix)?;
            for case in z_count_cases(loop_value, m, special_id) {
                if case.z_required < 0 {
                    continue;
                }
                let z_required = case.z_required as usize;
                if z_required > m {
                    continue;
                }
                if z_required < fixed_z {
                    continue;
                }
                let needed_z = z_required - fixed_z;
                if needed_z > free_suffix {
                    continue;
                }
                let ways_suffix = binom_i128(free_suffix, needed_z)?;
                total += case.sign * ways_prefix * ways_suffix;
            }
        }
    }
    Ok(total)
}

struct ZCountCase {
    z_required: i64,
    sign: i128,
}

fn z_count_cases(loop_value: usize, m: usize, special_id: usize) -> [ZCountCase; 2] {
    let l = loop_value as i64;
    let m = m as i64;
    if special_id == ID_ONE_MINUS_Z {
        [
            ZCountCase {
                z_required: l,
                sign: 1,
            },
            ZCountCase {
                z_required: l - 1,
                sign: -1,
            },
        ]
    } else {
        [
            ZCountCase {
                z_required: m - l,
                sign: -1,
            },
            ZCountCase {
                z_required: m - l + 1,
                sign: 1,
            },
        ]
    }
}

fn fixed_special_conflict(fixed: &[Option<usize>], special_pos: usize) -> bool {
    for (idx, value) in fixed.iter().enumerate() {
        if idx == special_pos {
            continue;
        }
        if let Some(id) = value {
            if *id >= ID_ONE_MINUS_Z {
                return true;
            }
        }
    }
    false
}

fn fixed_counts(
    fixed: &[Option<usize>],
    special_pos: usize,
    n: usize,
) -> Result<(usize, usize, usize, usize), ExperimentError> {
    let mut fixed_z = 0usize;
    let mut fixed_zbar = 0usize;
    let mut free_prefix = 0usize;

    for value in fixed.iter().take(special_pos) {
        match value {
            Some(ID_Z) | Some(ID_ZBAR) => {}
            Some(_) => {
                return Err(ExperimentError::InvalidConfig(
                    "special letter in prefix outside special position".to_string(),
                ))
            }
            None => free_prefix += 1,
        }
    }

    for value in fixed.iter().skip(special_pos + 1).take(n - special_pos - 1) {
        match value {
            Some(ID_Z) => fixed_z += 1,
            Some(ID_ZBAR) => fixed_zbar += 1,
            Some(_) => {
                return Err(ExperimentError::InvalidConfig(
                    "special letter in suffix outside special position".to_string(),
                ))
            }
            None => {}
        }
    }

    let m = n - special_pos - 1;
    let fixed_total = fixed_z + fixed_zbar;
    if fixed_total > m {
        return Err(ExperimentError::InvalidConfig(
            "fixed suffix count exceeds suffix length".to_string(),
        ));
    }
    let free_suffix = m - fixed_total;
    Ok((fixed_z, fixed_zbar, free_prefix, free_suffix))
}

fn coeff_from_i128(value: i128) -> Result<Coeff, ExperimentError> {
    if value > i64::MAX as i128 || value < i64::MIN as i128 {
        return Err(ExperimentError::InvalidConfig(
            "coefficient overflow (i64)".to_string(),
        ));
    }
    Ok(Coeff::from_integer(value as i64))
}

fn ladder_letter_exprs() -> Vec<Expr> {
    let z = Expr::Var("z".to_string()).normalize();
    let zbar = Expr::Var("zbar".to_string()).normalize();
    let one = Expr::Rational(Coeff::one());
    let one_minus_z = Expr::Add(vec![one.clone(), Expr::Neg(Box::new(z.clone()))]).normalize();
    let one_minus_zbar = Expr::Add(vec![
        Expr::Rational(Coeff::one()),
        Expr::Neg(Box::new(zbar.clone())),
    ])
    .normalize();
    vec![z, zbar, one_minus_z, one_minus_zbar]
}

fn ids_to_names(ids: &[usize]) -> Vec<String> {
    ids.iter().map(|&id| LETTER_NAMES[id].to_string()).collect()
}

fn enumerate_words(base: usize, len: usize) -> Vec<Vec<usize>> {
    if len == 0 {
        return vec![Vec::new()];
    }
    if base == 0 {
        return Vec::new();
    }
    let mut total = 1usize;
    for _ in 0..len {
        total = total.saturating_mul(base);
    }
    let mut out = Vec::with_capacity(total);
    let mut indices = vec![0usize; len];
    loop {
        out.push(indices.clone());
        let mut pos = len;
        while pos > 0 {
            pos -= 1;
            indices[pos] += 1;
            if indices[pos] < base {
                break;
            }
            indices[pos] = 0;
        }
        if pos == 0 && indices[0] == 0 {
            break;
        }
    }
    out
}

fn pow2_i128(exp: usize) -> Result<i128, ExperimentError> {
    if exp >= 127 {
        return Err(ExperimentError::InvalidConfig("2^exp overflow".to_string()));
    }
    Ok(1i128 << exp)
}

fn binom_i128(n: usize, k: usize) -> Result<i128, ExperimentError> {
    let value = binom_u128(n, k)?;
    if value > i128::MAX as u128 {
        return Err(ExperimentError::InvalidConfig(
            "binomial overflow".to_string(),
        ));
    }
    Ok(value as i128)
}

fn binom_u128(n: usize, k: usize) -> Result<u128, ExperimentError> {
    if k > n {
        return Ok(0);
    }
    let k = k.min(n - k);
    let mut value: u128 = 1;
    for i in 1..=k {
        let numerator = (n - k + i) as u128;
        value = value
            .checked_mul(numerator)
            .ok_or_else(|| ExperimentError::InvalidConfig("binomial overflow".to_string()))?;
        value /= i as u128;
    }
    Ok(value)
}

fn estimate_term_count(loop_value: usize) -> Option<u128> {
    let exp = loop_value.saturating_mul(2).saturating_add(1);
    if exp >= 128 {
        return None;
    }
    Some(1u128 << exp)
}

fn count_terms_exact(loop_value: usize) -> Result<u64, ExperimentError> {
    let mut total: u128 = 0;
    for p in 0..=loop_value {
        let m = loop_value
            .checked_mul(2)
            .and_then(|value| value.checked_sub(p))
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| ExperimentError::InvalidConfig("term count overflow".to_string()))?;
        let count_l = binom_u128(m, loop_value)?;
        let count_lm1 = binom_u128(m, loop_value.saturating_sub(1))?;
        let prefix = 1u128
            .checked_shl(p as u32)
            .ok_or_else(|| ExperimentError::InvalidConfig("prefix count overflow".to_string()))?;
        let add = (count_l + count_lm1)
            .checked_mul(prefix)
            .and_then(|value| value.checked_mul(2))
            .ok_or_else(|| ExperimentError::InvalidConfig("term count overflow".to_string()))?;
        total = total
            .checked_add(add)
            .ok_or_else(|| ExperimentError::InvalidConfig("term count overflow".to_string()))?;
    }
    if total > u64::MAX as u128 {
        return Err(ExperimentError::InvalidConfig(
            "term count overflow".to_string(),
        ));
    }
    Ok(total as u64)
}

fn for_each_ladder_term<F>(loop_value: usize, mut func: F) -> Result<u64, ExperimentError>
where
    F: FnMut(&[usize], Coeff) -> Result<(), ExperimentError>,
{
    if loop_value == 0 {
        return Ok(0);
    }
    let n = loop_value
        .checked_mul(2)
        .ok_or_else(|| ExperimentError::InvalidConfig("loop * 2 overflow".to_string()))?;
    let mut count = 0u64;
    for &special_id in &[ID_ONE_MINUS_Z, ID_ONE_MINUS_ZBAR] {
        for p in 0..=loop_value {
            if p >= n {
                continue;
            }
            let m = n - p - 1;
            let prefix_total = binary_total(p)?;
            for prefix_mask in 0..prefix_total {
                let prefix = binary_word_from_mask(p, prefix_mask);
                for case in z_count_cases(loop_value, m, special_id) {
                    if case.z_required < 0 {
                        continue;
                    }
                    let z_required = case.z_required as usize;
                    if z_required > m {
                        continue;
                    }
                    let suffix_total = binary_total(m)?;
                    for suffix_mask in 0..suffix_total {
                        let z_count = m - suffix_mask.count_ones() as usize;
                        if z_count != z_required {
                            continue;
                        }
                        let suffix = binary_word_from_mask(m, suffix_mask);
                        let mut word = Vec::with_capacity(n);
                        word.extend_from_slice(&prefix);
                        word.push(special_id);
                        word.extend_from_slice(&suffix);
                        func(&word, Coeff::from_integer(case.sign as i64))?;
                        count = count.saturating_add(1);
                    }
                }
            }
        }
    }
    Ok(count)
}

fn binary_total(len: usize) -> Result<u64, ExperimentError> {
    if len >= 63 {
        return Err(ExperimentError::InvalidConfig(
            "binary word length too large for jsonl emission".to_string(),
        ));
    }
    Ok(1u64 << len)
}

fn binary_word_from_mask(len: usize, mask: u64) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let shift = len - 1 - i;
        let bit = (mask >> shift) & 1;
        if bit == 0 {
            out.push(ID_Z);
        } else {
            out.push(ID_ZBAR);
        }
    }
    out
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
    merged_terms: u64,
}

#[derive(Serialize)]
struct TermLine {
    word: Vec<String>,
    coeff: String,
}

fn write_ladder_jsonl(
    path: &Path,
    loop_value: usize,
    max_terms: u64,
) -> Result<(), ExperimentError> {
    let estimate = estimate_term_count(loop_value).ok_or_else(|| {
        ExperimentError::InvalidConfig(format!("term estimate overflow for L={loop_value}"))
    })?;
    if estimate > max_terms as u128 {
        return Err(ExperimentError::InvalidConfig(format!(
            "loop {loop_value} estimated terms {estimate} exceed max_terms={max_terms}"
        )));
    }
    let merged_terms = count_terms_exact(loop_value)?;

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    let meta = MetaLine {
        meta: MetaContent {
            name: "Drummond2010Ladder",
            loop_index: loop_value,
            merged_terms,
        },
    };
    let meta_line = serde_json::to_string(&meta)
        .map_err(|err| ExperimentError::InvalidConfig(format!("json encode error: {err}")))?;
    writer.write_all(meta_line.as_bytes())?;
    writer.write_all(b"\n")?;

    for_each_ladder_term(loop_value, |ids, coeff| {
        let word = ids
            .iter()
            .map(|&id| LETTER_NAMES[id].to_string())
            .collect::<Vec<_>>();
        let line = TermLine {
            word,
            coeff: format_coeff(&coeff),
        };
        let encoded = serde_json::to_string(&line)
            .map_err(|err| ExperimentError::InvalidConfig(format!("json encode error: {err}")))?;
        writer.write_all(encoded.as_bytes())?;
        writer.write_all(b"\n")?;
        Ok(())
    })?;

    Ok(())
}

fn format_coeff(value: &Coeff) -> String {
    let numer = *value.numer();
    let denom = *value.denom();
    if denom == 1 {
        numer.to_string()
    } else {
        format!("{numer}/{denom}")
    }
}

fn strip_last(sym: &Symbol, letter: &Expr) -> Symbol {
    let mut out = Vec::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let letters = word.letters();
        if let Some(last) = letters.last() {
            if last == letter {
                let mut truncated = letters.to_vec();
                truncated.pop();
                out.push((Word(truncated), *coeff));
            }
        }
    }
    Symbol::from_terms(out)
}

fn symbol_one() -> Symbol {
    Symbol::from_terms(vec![(Word(Vec::new()), Coeff::one())])
}

fn symbol_scale(sym: &Symbol, coeff: Coeff) -> Symbol {
    if coeff.is_zero() {
        return Symbol::zero();
    }
    let mut terms = Vec::new();
    for (word, value) in sym.terms() {
        let scaled = *value * coeff;
        if !scaled.is_zero() {
            terms.push((word.clone(), scaled));
        }
    }
    Symbol::from_terms(terms)
}

fn symbol_add(left: &Symbol, right: &Symbol) -> Symbol {
    let mut terms = Vec::new();
    for (word, coeff) in left.terms() {
        terms.push((word.clone(), *coeff));
    }
    for (word, coeff) in right.terms() {
        terms.push((word.clone(), *coeff));
    }
    Symbol::from_terms(terms)
}

fn symbol_sub(left: &Symbol, right: &Symbol) -> Symbol {
    symbol_add(left, &symbol_scale(right, Coeff::from_integer(-1)))
}

fn li_symbol(n: usize, is_z: bool, exprs: &[Expr]) -> Symbol {
    if n == 0 {
        return Symbol::zero();
    }
    let special = if is_z {
        exprs[ID_ONE_MINUS_Z].clone()
    } else {
        exprs[ID_ONE_MINUS_ZBAR].clone()
    };
    let base = if is_z {
        exprs[ID_Z].clone()
    } else {
        exprs[ID_ZBAR].clone()
    };
    let mut letters = Vec::with_capacity(n);
    letters.push(special);
    for _ in 1..n {
        letters.push(base.clone());
    }
    Symbol::from_terms(vec![(Word(letters), Coeff::from_integer(-1))])
}

fn ladder_coeff(loop_value: usize, r: usize) -> Result<Coeff, ExperimentError> {
    let numer = factorial_i128(2 * loop_value - r)?;
    let denom = factorial_i128(r)? * factorial_i128(loop_value - r)? * factorial_i128(loop_value)?;
    let sign = if (loop_value + r).is_multiple_of(2) {
        1
    } else {
        -1
    };
    let numer = numer * sign as i128;
    let numer_i64 = i64::try_from(numer)
        .map_err(|_| ExperimentError::InvalidConfig("coefficient overflow".to_string()))?;
    let denom_i64 = i64::try_from(denom)
        .map_err(|_| ExperimentError::InvalidConfig("coefficient overflow".to_string()))?;
    Ok(Coeff::new(numer_i64, denom_i64))
}

fn factorial_i128(value: usize) -> Result<i128, ExperimentError> {
    let mut out: i128 = 1;
    for i in 2..=value {
        out = out
            .checked_mul(i as i128)
            .ok_or_else(|| ExperimentError::InvalidConfig("factorial overflow".to_string()))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn marginals_match_enumeration_small() {
        let loop_value = 3usize;
        let prefix_len = 2usize;
        let suffix_len = 2usize;
        let prefix_keys = enumerate_words(LETTER_NAMES.len(), prefix_len);
        let suffix_keys = enumerate_words(LETTER_NAMES.len(), suffix_len);
        let mut prefix_index = BTreeMap::new();
        let mut suffix_index = BTreeMap::new();
        for (idx, key) in prefix_keys.iter().enumerate() {
            prefix_index.insert(key.clone(), idx);
        }
        for (idx, key) in suffix_keys.iter().enumerate() {
            suffix_index.insert(key.clone(), idx);
        }

        let mut observed = vec![vec![Coeff::zero(); suffix_keys.len()]; prefix_keys.len()];
        let sym = ladder_symbol_combinatorial(loop_value).expect("symbol");
        let exprs = ladder_letter_exprs();
        for (word, coeff) in sym.terms() {
            let mut ids = Vec::new();
            for letter in word.letters() {
                let idx = exprs
                    .iter()
                    .position(|expr| expr == letter)
                    .expect("letter id");
                ids.push(idx);
            }
            let prefix = ids[..prefix_len].to_vec();
            let suffix = ids[ids.len() - suffix_len..].to_vec();
            let pidx = prefix_index[&prefix];
            let sidx = suffix_index[&suffix];
            observed[pidx][sidx] += *coeff;
        }

        for prefix in &prefix_keys {
            for suffix in &suffix_keys {
                let expected = ladder_marginal_count(loop_value, prefix, suffix).expect("count");
                let pidx = prefix_index[prefix];
                let sidx = suffix_index[suffix];
                assert_eq!(observed[pidx][sidx], expected);
            }
        }
    }
}
