use std::collections::BTreeMap;

use mpl_ir::{parse_sexpr, Expr};
use mpl_symbol::space::{
    build_integrable_basis_with_acceptor_with_stats_and_table, Alphabet, Basis, ConstraintBudget,
    SampleTable,
};
use mpl_symbol::{Coeff, Symbol, Word};
use num_traits::{One, Zero};

use crate::build::acceptors::{
    validate_automaton_order, validate_channel_pairs_acceptors, validate_genealogical_acceptors,
    validate_kgram_acceptors, CompositeAcceptor,
};
use crate::build::alphabet::{letter_display_names, normalize_inputs};
use crate::build::constraints::validate_constraints;
use crate::{ExperimentConfig, ExperimentError};

#[derive(Clone, Debug, Default)]
pub struct RowFilter {
    pub prefix: Option<Vec<Expr>>,
    pub max_rows: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct SuffixSpec {
    pub ids: Vec<usize>,
    pub letters: Vec<Expr>,
    pub names: Vec<String>,
}

impl SuffixSpec {
    pub fn from_names(alphabet: &Alphabet, names: &[String]) -> Result<Self, ExperimentError> {
        let ids = resolve_letter_ids(alphabet, names)?;
        let letters = ids
            .iter()
            .map(|idx| alphabet.letters[*idx].clone())
            .collect::<Vec<_>>();
        let display_names = letter_display_names(alphabet);
        let resolved_names = ids
            .iter()
            .map(|idx| {
                display_names
                    .get(*idx)
                    .cloned()
                    .unwrap_or_else(|| idx.to_string())
            })
            .collect();
        Ok(Self {
            ids,
            letters,
            names: resolved_names,
        })
    }
}

pub fn prefix_from_names(
    alphabet: &Alphabet,
    names: &[String],
) -> Result<Vec<Expr>, ExperimentError> {
    let ids = resolve_letter_ids(alphabet, names)?;
    Ok(ids
        .into_iter()
        .map(|idx| alphabet.letters[idx].clone())
        .collect())
}

#[derive(Clone, Debug)]
pub struct CrossLoopOptions {
    pub weight: usize,
    pub lower_weight: Option<usize>,
    pub suffix: SuffixSpec,
    pub row_filter: RowFilter,
    pub residual_word_limit: usize,
    pub compute_mapping: bool,
}

#[derive(Clone, Debug)]
pub struct CrossLoopScanOptions {
    pub weight_min: usize,
    pub weight_max: usize,
    pub suffix: SuffixSpec,
    pub suffix_index: usize,
    pub suffix_total: usize,
    pub row_filter: RowFilter,
    pub residual_word_limit: usize,
    pub compute_mapping: bool,
    pub prefactor_col: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ImageRankReport {
    pub rank: usize,
    pub row_count: usize,
    pub zero_columns: Vec<usize>,
    pub pivot_columns: Vec<usize>,
    pub row_limit_hit: bool,
}

#[derive(Clone, Debug)]
pub struct ResidualSummary {
    pub column: usize,
    pub word_count: usize,
    pub sample_words: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RankOneFactor {
    pub u: Vec<Coeff>,
    pub v: Vec<Coeff>,
    pub normalized_row: usize,
}

#[derive(Clone, Debug)]
pub struct MappingReport {
    pub rank: usize,
    pub success_cols: Vec<usize>,
    pub failed_cols: Vec<usize>,
    pub residual_rank: usize,
    pub residual_row_count: usize,
    pub residuals: Vec<ResidualSummary>,
    pub matrix: Vec<Vec<Coeff>>,
    pub rank_one: Option<RankOneFactor>,
}

#[derive(Clone, Debug)]
pub struct CrossLoopReport {
    pub weight: usize,
    pub lower_weight: usize,
    pub suffix: SuffixSpec,
    pub upper_dim: usize,
    pub lower_dim: usize,
    pub image_rank: ImageRankReport,
    pub mapping: Option<MappingReport>,
}

#[derive(Clone, Debug)]
pub struct CrossLoopScanRow {
    pub weight: usize,
    pub image_rank: usize,
    pub row_count: usize,
    pub zero_columns: usize,
    pub mapping_rank: Option<usize>,
    pub mapping_failed: Option<usize>,
    pub rank_one: bool,
    pub prefactor_col: Option<usize>,
    pub prefactor_value: Option<Coeff>,
}

#[derive(Clone, Debug)]
pub struct CrossLoopScanFit {
    pub model: String,
    pub scale: Coeff,
}

#[derive(Clone, Debug)]
pub struct CrossLoopScanReport {
    pub suffix: SuffixSpec,
    pub suffix_index: usize,
    pub suffix_total: usize,
    pub rows: Vec<CrossLoopScanRow>,
    pub fits: Vec<CrossLoopScanFit>,
}

pub fn run_cross_loop(
    cfg: &ExperimentConfig,
    options: &CrossLoopOptions,
) -> Result<CrossLoopReport, ExperimentError> {
    let (alphabet, constraints) = normalize_inputs(&cfg.alphabet, &cfg.constraints);
    validate_constraints(&alphabet, &constraints)?;
    validate_genealogical_acceptors(&alphabet, &cfg.genealogical_acceptors)?;
    validate_kgram_acceptors(&alphabet, &cfg.kgram_acceptors)?;
    validate_channel_pairs_acceptors(&alphabet, &cfg.channel_pairs_acceptors)?;
    validate_automaton_order(
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
        &cfg.channel_pairs_acceptors,
    )?;

    let acceptor = CompositeAcceptor::new(
        &constraints,
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
        &cfg.channel_pairs_acceptors,
    );
    let upper_basis = build_basis(
        &alphabet,
        &acceptor,
        cfg.constraint_budget,
        cfg.sample_table,
        options.weight,
    )?;
    let suffix_cache = build_suffix_cache(&upper_basis.words, &options.suffix.ids, &alphabet)?;
    let truncated = project_basis_with_cache(&upper_basis, &suffix_cache);
    let image_rank = image_rank(&truncated, &options.row_filter);

    let lower_weight = options
        .lower_weight
        .unwrap_or_else(|| options.weight.saturating_sub(options.suffix.ids.len()));

    let (mapping, lower_dim) = if options.compute_mapping {
        let lower_basis = build_basis(
            &alphabet,
            &acceptor,
            cfg.constraint_budget,
            cfg.sample_table,
            lower_weight,
        )?;
        let lower_dim = lower_basis.vectors.len();
        let report = express_images_in_lower_space(
            &truncated,
            &lower_basis,
            &alphabet,
            options.residual_word_limit,
        )?;
        (Some(report), lower_dim)
    } else {
        (None, 0)
    };

    Ok(CrossLoopReport {
        weight: options.weight,
        lower_weight,
        suffix: options.suffix.clone(),
        upper_dim: upper_basis.vectors.len(),
        lower_dim,
        image_rank,
        mapping,
    })
}

pub fn run_cross_loop_scan(
    cfg: &ExperimentConfig,
    options: &CrossLoopScanOptions,
) -> Result<CrossLoopScanReport, ExperimentError> {
    if options.weight_min > options.weight_max {
        return Err(ExperimentError::InvalidConfig(
            "weight_min must be <= weight_max".to_string(),
        ));
    }
    if options.suffix_total == 0 || options.suffix_index >= options.suffix_total {
        return Err(ExperimentError::InvalidConfig(
            "suffix index out of range".to_string(),
        ));
    }

    let (alphabet, constraints) = normalize_inputs(&cfg.alphabet, &cfg.constraints);
    validate_constraints(&alphabet, &constraints)?;
    validate_genealogical_acceptors(&alphabet, &cfg.genealogical_acceptors)?;
    validate_kgram_acceptors(&alphabet, &cfg.kgram_acceptors)?;
    validate_channel_pairs_acceptors(&alphabet, &cfg.channel_pairs_acceptors)?;
    validate_automaton_order(
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
        &cfg.channel_pairs_acceptors,
    )?;

    let acceptor = CompositeAcceptor::new(
        &constraints,
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
        &cfg.channel_pairs_acceptors,
    );

    let mut rows = Vec::new();
    let mut prefactor_points = Vec::new();
    let mut selected_prefactor_col = options.prefactor_col;

    for weight in options.weight_min..=options.weight_max {
        let upper_basis = build_basis(
            &alphabet,
            &acceptor,
            cfg.constraint_budget,
            cfg.sample_table,
            weight,
        )?;
        let suffix_cache = build_suffix_cache(&upper_basis.words, &options.suffix.ids, &alphabet)?;
        let truncated = project_basis_with_cache(&upper_basis, &suffix_cache);
        let image_rank = image_rank(&truncated, &options.row_filter);

        let mapping = if options.compute_mapping {
            let lower_weight = weight.saturating_sub(options.suffix.ids.len());
            let lower_basis = build_basis(
                &alphabet,
                &acceptor,
                cfg.constraint_budget,
                cfg.sample_table,
                lower_weight,
            )?;
            Some(express_images_in_lower_space(
                &truncated,
                &lower_basis,
                &alphabet,
                options.residual_word_limit,
            )?)
        } else {
            None
        };

        let mut prefactor_value = None;
        let mut prefactor_col = None;
        let mut rank_one = false;
        let mut mapping_rank = None;
        let mut mapping_failed = None;

        if let Some(report) = mapping.as_ref() {
            mapping_rank = Some(report.rank);
            mapping_failed = Some(report.failed_cols.len());
            rank_one = report.rank == 1 && report.rank_one.is_some();
            if let Some(factor) = report.rank_one.as_ref() {
                if selected_prefactor_col.is_none() {
                    selected_prefactor_col = select_prefactor_col(factor, options.prefactor_col);
                }
                if let Some(target_col) = selected_prefactor_col {
                    prefactor_col = Some(target_col);
                    let value = factor.v.get(target_col).copied();
                    if let Some(value) = value {
                        prefactor_value = Some(value);
                        prefactor_points.push((weight as i64, value));
                    }
                }
            }
        }

        rows.push(CrossLoopScanRow {
            weight,
            image_rank: image_rank.rank,
            row_count: image_rank.row_count,
            zero_columns: image_rank.zero_columns.len(),
            mapping_rank,
            mapping_failed,
            rank_one,
            prefactor_col,
            prefactor_value,
        });
    }

    let fits = fit_prefactor_models(&prefactor_points);

    Ok(CrossLoopScanReport {
        suffix: options.suffix.clone(),
        suffix_index: options.suffix_index,
        suffix_total: options.suffix_total,
        rows,
        fits,
    })
}

pub fn image_rank(basis: &[Symbol], filter: &RowFilter) -> ImageRankReport {
    let (rows, row_limit_hit, zero_columns) = build_row_matrix(basis, filter);
    let (rank, pivot_columns) = rank_from_rows(&rows);
    ImageRankReport {
        rank,
        row_count: rows.len(),
        zero_columns,
        pivot_columns,
        row_limit_hit,
    }
}

pub fn express_images_in_lower_space(
    images: &[Symbol],
    lower_basis: &Basis,
    alphabet: &Alphabet,
    residual_word_limit: usize,
) -> Result<MappingReport, ExperimentError> {
    let mut coeffs_by_col = Vec::with_capacity(images.len());
    let mut failed_cols = Vec::new();
    let mut residuals = Vec::new();
    let mut residual_symbols = Vec::new();
    let mut success_mask = vec![false; images.len()];

    for (col, sym) in images.iter().enumerate() {
        let (coeffs, residual) = mpl_symbol::space::reduce_to_basis(sym, lower_basis, alphabet)?;
        let mut is_zero = true;
        let mut word_count = 0usize;
        let mut samples = Vec::new();
        for (word, coeff) in residual.terms() {
            if coeff.is_zero() {
                continue;
            }
            is_zero = false;
            word_count += 1;
            if samples.len() < residual_word_limit {
                samples.push(word.to_string());
            }
        }
        if !is_zero {
            failed_cols.push(col);
            residuals.push(ResidualSummary {
                column: col,
                word_count,
                sample_words: samples,
            });
            residual_symbols.push(residual);
        } else {
            success_mask[col] = true;
        }
        coeffs_by_col.push(coeffs);
    }

    let lower_dim = lower_basis.vectors.len();
    let upper_dim = images.len();
    let mut matrix = vec![vec![Coeff::zero(); upper_dim]; lower_dim];
    for (col, coeffs) in coeffs_by_col.iter().enumerate() {
        for (row, coeff) in coeffs.iter().enumerate() {
            if row < lower_dim {
                matrix[row][col] = *coeff;
            }
        }
    }

    let rank = rank_from_matrix(&matrix, Some(&success_mask));

    let (residual_rows, residual_rank) = if failed_cols.is_empty() {
        (0, 0)
    } else {
        let (rows, _limit_hit, _zero_cols) =
            build_row_matrix(&residual_symbols, &RowFilter::default());
        let (rank, _pivots) = rank_from_rows(&rows);
        (rows.len(), rank)
    };

    let rank_one = if rank == 1 {
        rank_one_factor(&matrix, Some(&success_mask))
    } else {
        None
    };

    let success_cols = success_mask
        .iter()
        .enumerate()
        .filter_map(|(idx, ok)| if *ok { Some(idx) } else { None })
        .collect::<Vec<_>>();

    Ok(MappingReport {
        rank,
        success_cols,
        failed_cols,
        residual_rank,
        residual_row_count: residual_rows,
        residuals,
        matrix,
        rank_one,
    })
}

fn build_basis(
    alphabet: &Alphabet,
    acceptor: &CompositeAcceptor<'_>,
    budget: ConstraintBudget,
    sample_table: SampleTable,
    weight: usize,
) -> Result<Basis, ExperimentError> {
    build_integrable_basis_with_acceptor_with_stats_and_table(
        alphabet,
        acceptor,
        weight,
        Some(&budget),
        sample_table,
    )
    .map_err(|err| err.err.into())
}

fn build_suffix_cache(
    words: &[Vec<usize>],
    suffix_ids: &[usize],
    alphabet: &Alphabet,
) -> Result<Vec<Option<Word>>, ExperimentError> {
    let suffix_len = suffix_ids.len();
    let mut cache = Vec::with_capacity(words.len());
    for ids in words {
        if ids.len() < suffix_len || !ids.ends_with(suffix_ids) {
            cache.push(None);
            continue;
        }
        let prefix_len = ids.len() - suffix_len;
        let word = ids_to_word(&ids[..prefix_len], alphabet)?;
        cache.push(Some(word));
    }
    Ok(cache)
}

fn project_basis_with_cache(basis: &Basis, cache: &[Option<Word>]) -> Vec<Symbol> {
    let mut out = Vec::with_capacity(basis.vectors.len());
    for vec in &basis.vectors {
        let mut terms = Vec::new();
        for (col, coeff) in vec.iter().enumerate() {
            if coeff.is_zero() {
                continue;
            }
            let Some(word) = cache.get(col).and_then(|value| value.as_ref()) else {
                continue;
            };
            terms.push((word.clone(), *coeff));
        }
        out.push(Symbol::from_terms(terms));
    }
    out
}

fn ids_to_word(ids: &[usize], alphabet: &Alphabet) -> Result<Word, ExperimentError> {
    let mut letters = Vec::with_capacity(ids.len());
    for &id in ids {
        let letter = alphabet.letters.get(id).cloned().ok_or_else(|| {
            ExperimentError::InvalidConfig("basis refers to missing alphabet letter".to_string())
        })?;
        letters.push(letter);
    }
    Ok(Word(letters))
}

fn resolve_letter_ids(
    alphabet: &Alphabet,
    names: &[String],
) -> Result<Vec<usize>, ExperimentError> {
    if names.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "letter list must be non-empty".to_string(),
        ));
    }
    let display_names = letter_display_names(alphabet);
    let mut name_to_idx: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, name) in display_names.iter().enumerate() {
        name_to_idx.insert(name.clone(), idx);
    }
    let mut expr_to_idx = BTreeMap::new();
    for (idx, letter) in alphabet.letters.iter().enumerate() {
        expr_to_idx.insert(letter.normalize().to_canonical_string(), idx);
    }

    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        if let Some(idx) = name_to_idx.get(name) {
            ids.push(*idx);
            continue;
        }
        let expr = parse_sexpr(name)
            .map_err(|err| ExperimentError::InvalidConfig(format!("suffix parse error: {err}")))?;
        let key = expr.normalize().to_canonical_string();
        let idx = expr_to_idx.get(&key).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!("unknown suffix letter: {name}"))
        })?;
        ids.push(*idx);
    }
    Ok(ids)
}

fn build_row_matrix(
    basis: &[Symbol],
    filter: &RowFilter,
) -> (BTreeMap<Word, SparseRow>, bool, Vec<usize>) {
    let mut rows: BTreeMap<Word, SparseRow> = BTreeMap::new();
    let mut zero_columns = vec![true; basis.len()];
    let mut row_limit_hit = false;

    let prefix_keys = filter.prefix.as_ref().map(|letters| {
        letters
            .iter()
            .map(|e| e.normalize().to_canonical_string())
            .collect::<Vec<String>>()
    });

    for (col, sym) in basis.iter().enumerate() {
        for (word, coeff) in sym.terms() {
            if coeff.is_zero() {
                continue;
            }
            if let Some(keys) = prefix_keys.as_ref() {
                if !prefix_matches(word, keys) {
                    continue;
                }
            }
            if let Some(limit) = filter.max_rows {
                if rows.len() >= limit && !rows.contains_key(word) {
                    row_limit_hit = true;
                    continue;
                }
            }
            zero_columns[col] = false;
            let row = rows.entry(word.clone()).or_default();
            let updated = row.get(&col).copied().unwrap_or_else(Coeff::zero) + *coeff;
            if updated.is_zero() {
                row.remove(&col);
            } else {
                row.insert(col, updated);
            }
        }
    }

    let zero_cols = zero_columns
        .iter()
        .enumerate()
        .filter_map(|(idx, is_zero)| if *is_zero { Some(idx) } else { None })
        .collect();

    (rows, row_limit_hit, zero_cols)
}

fn prefix_matches(word: &Word, prefix_keys: &[String]) -> bool {
    if prefix_keys.is_empty() {
        return true;
    }
    let letters = word.letters();
    if letters.len() < prefix_keys.len() {
        return false;
    }
    for (idx, key) in prefix_keys.iter().enumerate() {
        if letters[idx].to_canonical_string() != *key {
            return false;
        }
    }
    true
}

type SparseRow = BTreeMap<usize, Coeff>;

fn rank_from_rows(rows: &BTreeMap<Word, SparseRow>) -> (usize, Vec<usize>) {
    let mut pivot_rows: BTreeMap<usize, SparseRow> = BTreeMap::new();
    let mut pivot_columns = Vec::new();
    for row in rows.values() {
        let row = row.clone();
        if let Some(pivot) = insert_row(&mut pivot_rows, row) {
            pivot_columns.push(pivot);
        }
    }
    (pivot_rows.len(), pivot_columns)
}

fn rank_from_matrix(matrix: &[Vec<Coeff>], col_mask: Option<&[bool]>) -> usize {
    let mut pivot_rows: BTreeMap<usize, SparseRow> = BTreeMap::new();
    for row in matrix {
        let mut sparse = SparseRow::new();
        for (col_idx, coeff) in row.iter().enumerate() {
            if let Some(mask) = col_mask {
                if !mask.get(col_idx).copied().unwrap_or(false) {
                    continue;
                }
            }
            if !coeff.is_zero() {
                sparse.insert(col_idx, *coeff);
            }
        }
        if !sparse.is_empty() {
            let _ = insert_row(&mut pivot_rows, sparse);
        }
    }
    pivot_rows.len()
}

fn insert_row(pivot_rows: &mut BTreeMap<usize, SparseRow>, mut row: SparseRow) -> Option<usize> {
    loop {
        let pivot_col = row.keys().next().copied()?;
        if let Some(existing) = pivot_rows.get(&pivot_col) {
            let factor = row.get(&pivot_col).copied().unwrap_or_else(Coeff::zero);
            let existing = existing.clone();
            add_scaled_row(&mut row, factor, &existing);
            continue;
        }

        let pivot = row.get(&pivot_col).copied().unwrap_or_else(Coeff::zero);
        if !pivot.is_zero() {
            let inv = Coeff::one() / pivot;
            scale_row(&mut row, inv);
        }
        pivot_rows.insert(pivot_col, row);
        return Some(pivot_col);
    }
}

fn add_scaled_row(row: &mut SparseRow, factor: Coeff, other: &SparseRow) {
    if factor.is_zero() {
        return;
    }
    for (col, value) in other {
        let updated = row.get(col).copied().unwrap_or_else(Coeff::zero) - factor * *value;
        if updated.is_zero() {
            row.remove(col);
        } else {
            row.insert(*col, updated);
        }
    }
}

fn scale_row(row: &mut SparseRow, factor: Coeff) {
    if factor.is_one() {
        return;
    }
    let keys: Vec<usize> = row.keys().copied().collect();
    for col in keys {
        let updated = row.get(&col).copied().unwrap_or_else(Coeff::zero) * factor;
        if updated.is_zero() {
            row.remove(&col);
        } else {
            row.insert(col, updated);
        }
    }
}

fn rank_one_factor(matrix: &[Vec<Coeff>], col_mask: Option<&[bool]>) -> Option<RankOneFactor> {
    if matrix.is_empty() {
        return None;
    }
    let nrows = matrix.len();
    let ncols = matrix[0].len();
    let mut seed_vec: Option<Vec<Coeff>> = None;
    let mut pivot_row = 0usize;

    for col in 0..ncols {
        if let Some(mask) = col_mask {
            if !mask.get(col).copied().unwrap_or(false) {
                continue;
            }
        }
        let mut col_vec = vec![Coeff::zero(); nrows];
        let mut any = false;
        for row in 0..nrows {
            let coeff = matrix[row][col];
            col_vec[row] = coeff;
            any = any || !coeff.is_zero();
        }
        if any {
            pivot_row = col_vec
                .iter()
                .enumerate()
                .find(|(_, coeff)| !coeff.is_zero())
                .map(|(idx, _)| idx)?;
            let pivot = col_vec[pivot_row];
            if pivot.is_zero() {
                return None;
            }
            let inv = Coeff::one() / pivot;
            for value in &mut col_vec {
                *value *= inv;
            }
            seed_vec = Some(col_vec);
            break;
        }
    }

    let seed_vec = seed_vec?;

    let mut v = vec![Coeff::zero(); ncols];
    for col in 0..ncols {
        if let Some(mask) = col_mask {
            if !mask.get(col).copied().unwrap_or(false) {
                continue;
            }
        }
        let scale = matrix[pivot_row][col];
        if scale.is_zero() {
            for row in 0..nrows {
                if !matrix[row][col].is_zero() {
                    return None;
                }
            }
            v[col] = Coeff::zero();
            continue;
        }
        for row in 0..nrows {
            if matrix[row][col] != seed_vec[row] * scale {
                return None;
            }
        }
        v[col] = scale;
    }

    let mut u = seed_vec;
    u.resize(nrows, Coeff::zero());

    Some(RankOneFactor {
        u,
        v,
        normalized_row: pivot_row,
    })
}

fn select_prefactor_col(factor: &RankOneFactor, requested: Option<usize>) -> Option<usize> {
    if let Some(idx) = requested {
        if idx < factor.v.len() {
            return Some(idx);
        }
    }
    for (idx, value) in factor.v.iter().enumerate() {
        if !value.is_zero() {
            return Some(idx);
        }
    }
    None
}

fn fit_prefactor_models(points: &[(i64, Coeff)]) -> Vec<CrossLoopScanFit> {
    let mut out = Vec::new();
    if points.len() < 2 {
        return out;
    }

    if let Some((ratio, scale)) = fit_geometric(points) {
        out.push(CrossLoopScanFit {
            model: format!("geom(r={})", format_coeff(&ratio)),
            scale,
        });
    }

    for degree in 0..=2 {
        if let Some(coeffs) = fit_polynomial(points, degree) {
            let model = format!("poly_deg_{degree}({})", format_coeffs(&coeffs));
            out.push(CrossLoopScanFit {
                model,
                scale: Coeff::one(),
            });
        }
    }

    for (name, func) in candidate_library() {
        if let Some(scale) = fit_scaled_sequence(points, &func) {
            out.push(CrossLoopScanFit {
                model: name.to_string(),
                scale,
            });
        }
    }

    out
}

fn fit_geometric(points: &[(i64, Coeff)]) -> Option<(Coeff, Coeff)> {
    let mut ratio = None;
    let mut last: Option<(i64, Coeff)> = None;
    for (x, y) in points {
        if let Some((prev_x, prev_y)) = last {
            let delta = x - prev_x;
            if delta != 1 {
                return None;
            }
            if prev_y.is_zero() {
                return None;
            }
            let r = *y / prev_y;
            match ratio {
                Some(value) if value != r => return None,
                None => ratio = Some(r),
                _ => {}
            }
        }
        last = Some((*x, *y));
    }
    let ratio = ratio?;
    let (x0, y0) = points.first().copied()?;
    let base = pow_coeff(ratio, x0)?;
    if base.is_zero() {
        return None;
    }
    let scale = y0 / base;
    Some((ratio, scale))
}

fn fit_polynomial(points: &[(i64, Coeff)], degree: usize) -> Option<Vec<Coeff>> {
    if points.len() < degree + 1 {
        return None;
    }
    let n = degree + 1;
    let mut matrix = vec![vec![Coeff::zero(); n]; n];
    let mut rhs = vec![Coeff::zero(); n];

    for (row, (x, y)) in points.iter().take(n).enumerate() {
        rhs[row] = *y;
        let mut pow = Coeff::one();
        for col in 0..n {
            matrix[row][col] = pow;
            pow *= Coeff::from_integer(*x);
        }
    }

    let coeffs = solve_square_system(&matrix, &rhs)?;

    for (x, y) in points.iter() {
        let mut pow = Coeff::one();
        let mut predicted = Coeff::zero();
        for coeff in &coeffs {
            predicted += *coeff * pow;
            pow *= Coeff::from_integer(*x);
        }
        if predicted != *y {
            return None;
        }
    }

    Some(coeffs)
}

fn solve_square_system(matrix: &[Vec<Coeff>], rhs: &[Coeff]) -> Option<Vec<Coeff>> {
    let n = matrix.len();
    if n == 0 || rhs.len() != n {
        return None;
    }
    let mut a = matrix.to_vec();
    let mut b = rhs.to_vec();
    for row in &a {
        if row.len() != n {
            return None;
        }
    }

    for col in 0..n {
        let mut pivot = None;
        for row in col..n {
            if !a[row][col].is_zero() {
                pivot = Some(row);
                break;
            }
        }
        let pivot = pivot?;
        if pivot != col {
            a.swap(pivot, col);
            b.swap(pivot, col);
        }
        let inv = Coeff::one() / a[col][col];
        for value in &mut a[col] {
            *value *= inv;
        }
        b[col] *= inv;

        let pivot_row_vals = a[col].clone();
        let pivot_b = b[col];
        for row in (col + 1)..n {
            let factor = a[row][col];
            if factor.is_zero() {
                continue;
            }
            for idx in col..n {
                a[row][idx] -= factor * pivot_row_vals[idx];
            }
            b[row] -= factor * pivot_b;
        }
    }

    let mut solution = vec![Coeff::zero(); n];
    for row in (0..n).rev() {
        let mut sum = Coeff::zero();
        for col in (row + 1)..n {
            sum += a[row][col] * solution[col];
        }
        solution[row] = b[row] - sum;
    }

    Some(solution)
}

fn fit_scaled_sequence(
    points: &[(i64, Coeff)],
    candidate: &dyn Fn(i64) -> Option<Coeff>,
) -> Option<Coeff> {
    let mut scale = None;
    for (x, y) in points {
        let value = candidate(*x)?;
        if value.is_zero() {
            if !y.is_zero() {
                return None;
            }
            continue;
        }
        let ratio = *y / value;
        match scale {
            Some(existing) if existing != ratio => return None,
            None => scale = Some(ratio),
            _ => {}
        }
    }
    scale
}

fn candidate_library() -> Vec<(&'static str, Box<dyn Fn(i64) -> Option<Coeff>>)> {
    vec![
        ("seq_one", Box::new(|_| Some(Coeff::one()))),
        (
            "seq_pow2",
            Box::new(|n| pow_i64(2, n).map(Coeff::from_integer)),
        ),
        (
            "seq_factorial",
            Box::new(|n| factorial(n).map(Coeff::from_integer)),
        ),
        (
            "seq_double_fact_odd",
            Box::new(|n| double_factorial_odd(2 * n - 1).map(Coeff::from_integer)),
        ),
    ]
}

fn pow_i64(base: i64, exp: i64) -> Option<i64> {
    if exp < 0 {
        return None;
    }
    let mut out = 1i64;
    for _ in 0..exp {
        out = out.checked_mul(base)?;
    }
    Some(out)
}

fn pow_coeff(base: Coeff, exp: i64) -> Option<Coeff> {
    if exp < 0 {
        return None;
    }
    let mut out = Coeff::one();
    for _ in 0..exp {
        out *= base;
    }
    Some(out)
}

fn factorial(n: i64) -> Option<i64> {
    if n < 0 {
        return None;
    }
    let mut out = 1i64;
    for k in 2..=n {
        out = out.checked_mul(k)?;
    }
    Some(out)
}

fn double_factorial_odd(n: i64) -> Option<i64> {
    if n < 0 {
        return None;
    }
    let mut out = 1i64;
    let mut k = if n % 2 == 0 { n - 1 } else { n };
    while k >= 1 {
        out = out.checked_mul(k)?;
        k -= 2;
    }
    Some(out)
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

fn format_coeffs(values: &[Coeff]) -> String {
    values
        .iter()
        .map(format_coeff)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpl_symbol::space::{build_integrable_basis, WordConstraints};
    use num_traits::{One, Zero};

    fn r(value: i64) -> Coeff {
        Coeff::from_integer(value)
    }

    #[test]
    fn image_rank_and_mapping_basic() {
        let alphabet = crate::build::alphabet::toy_alphabet_xy();
        let constraints = WordConstraints::default();
        let lower_basis = build_integrable_basis(&alphabet, &constraints, 1).expect("basis");

        let x = alphabet.letters[0].clone();
        let y = alphabet.letters[1].clone();

        let b1 = Symbol::from_terms(vec![(Word(vec![x.clone(), x.clone()]), r(1))]);
        let b2 = Symbol::from_terms(vec![(Word(vec![y.clone(), x.clone()]), r(1))]);
        let b3 = Symbol::from_terms(vec![(Word(vec![x.clone(), y.clone()]), r(1))]);

        let basis = vec![b1, b2, b3];
        let suffix = vec![x.clone()];
        let truncated = mpl_symbol::apply_suffix_projection_to_basis(&basis, &suffix);

        let report = image_rank(&truncated, &RowFilter::default());
        assert_eq!(report.rank, 2);
        assert_eq!(report.row_count, 2);
        assert_eq!(report.zero_columns, vec![2]);

        let mapping =
            express_images_in_lower_space(&truncated, &lower_basis, &alphabet, 4).expect("map");
        assert_eq!(mapping.rank, 2);
        assert!(mapping.failed_cols.is_empty());
        assert_eq!(mapping.matrix.len(), 2);
        assert_eq!(mapping.matrix[0].len(), 3);
        assert_eq!(mapping.matrix[0][0], Coeff::one());
        assert!(mapping.matrix[0][1].is_zero());
        assert!(mapping.matrix[0][2].is_zero());
        assert!(mapping.matrix[1][0].is_zero());
        assert_eq!(mapping.matrix[1][1], Coeff::one());
        assert!(mapping.matrix[1][2].is_zero());
    }
}
