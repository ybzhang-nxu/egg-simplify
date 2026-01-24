use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use mpl_symbol::Coeff;
use num_traits::Zero;

use crate::output::csv::CsvWriter;
use crate::ExperimentError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanFamilyFilter {
    All,
    Prefix,
    Suffix,
    PrefixSuffix,
}

impl SpanFamilyFilter {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Prefix => "prefix",
            Self::Suffix => "suffix",
            Self::PrefixSuffix => "prefix-suffix",
        }
    }

    fn matches(self, family: &str) -> bool {
        match self {
            Self::All => true,
            Self::Prefix => family == "prefix",
            Self::Suffix => family == "suffix",
            Self::PrefixSuffix => family == "prefix-suffix",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoefSet {
    Pm1,
    Pm2,
}

impl CoefSet {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pm1 => "pm1",
            Self::Pm2 => "pm2",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EsymbSpanDepsConfig {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub family: SpanFamilyFilter,
    pub support_max: usize,
    pub coef_set: CoefSet,
    pub top_k: usize,
    pub export_forbidden: bool,
    pub export_equiv_classes: bool,
}

#[derive(Clone, Debug)]
struct Observable {
    family: String,
    params: String,
    id: String,
    order: usize,
    values: Vec<Coeff>,
    is_trivial: bool,
}

type EquivGroup = (String, Coeff, Vec<Coeff>);
type EquivGroups = BTreeMap<Vec<Coeff>, Vec<EquivGroup>>;
type KeyList = Vec<(String, String)>;
type BasisKeyList = Vec<BasisKey>;
type BasisExpansionList = Vec<BasisExpansion>;
type SupportMaskList = Vec<SupportMaskRow>;
type MaskHistogramList = Vec<MaskHistogramRow>;
type AllowedEdgeList = Vec<AllowedEdge>;

#[derive(Clone, Debug)]
pub struct SpanStats {
    pub family: String,
    pub total: usize,
    pub nonzero: usize,
    pub trivial: usize,
    pub rank: usize,
    pub nullity: usize,
}

#[derive(Clone, Debug)]
pub struct EquivMember {
    pub id: String,
    pub ratio: Coeff,
}

#[derive(Clone, Debug)]
pub struct EquivClass {
    pub family: String,
    pub representative: String,
    pub members: Vec<EquivMember>,
}

#[derive(Clone, Debug)]
pub struct SparseRelation {
    pub family: String,
    pub terms: Vec<(i32, String)>,
}

#[derive(Clone, Debug)]
pub struct SparseSearchStats {
    pub family: String,
    pub attempts: usize,
    pub hits_mod_p: usize,
    pub verified: usize,
}

#[derive(Clone, Debug)]
pub struct BasisKey {
    pub family: String,
    pub key: String,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct BasisExpansion {
    pub family: String,
    pub key: String,
    pub prime: i64,
    pub coeffs: Vec<i64>,
}

#[derive(Clone, Debug)]
pub struct SupportMaskRow {
    pub key: String,
    pub bitmask: u64,
    pub nnz: usize,
}

#[derive(Clone, Debug)]
pub struct MaskHistogramRow {
    pub bitmask: u64,
    pub count: usize,
}

#[derive(Clone, Debug)]
pub struct AllowedEdge {
    pub prefix: String,
    pub suffix: String,
}

#[derive(Clone, Debug)]
pub struct SpanDepsReport {
    pub loops: Vec<usize>,
    pub stats: Vec<SpanStats>,
    pub equiv_classes: Vec<EquivClass>,
    pub relations: Vec<SparseRelation>,
    pub search_stats: Vec<SparseSearchStats>,
    pub basis_keys: BasisKeyList,
    pub mask_histogram: MaskHistogramList,
    pub total_observables: usize,
    pub trivial_observables: usize,
}

struct BasisModP {
    keys: BasisKeyList,
    expansions: BasisExpansionList,
}

pub fn run_esymb_span_deps(cfg: &EsymbSpanDepsConfig) -> Result<SpanDepsReport, ExperimentError> {
    if cfg.support_max < 2 {
        return Err(ExperimentError::InvalidConfig(
            "--support-max must be >= 2".to_string(),
        ));
    }
    if cfg.top_k == 0 {
        return Err(ExperimentError::InvalidConfig(
            "--top-k must be >= 1".to_string(),
        ));
    }
    if cfg.coef_set == CoefSet::Pm2 && cfg.support_max < 2 {
        return Err(ExperimentError::InvalidConfig(
            "coef-set pm2 requires support-max >= 2".to_string(),
        ));
    }

    let (loops, mut observables) = read_observables_csv(&cfg.input)?;
    observables.retain(|obs| cfg.family.matches(&obs.family));
    let total_observables = observables.len();
    let order_map = build_order_map(&observables)?;

    let mut by_family: BTreeMap<String, Vec<Observable>> = BTreeMap::new();
    let mut trivial_observables = 0usize;
    for obs in observables {
        if obs.is_trivial {
            trivial_observables += 1;
        }
        by_family.entry(obs.family.clone()).or_default().push(obs);
    }
    for items in by_family.values_mut() {
        items.sort_by(|a, b| compare_observable(&order_map, a, b));
    }

    let mut stats = Vec::new();
    let mut equiv_classes = Vec::new();
    let mut relations = Vec::new();
    let mut search_stats = Vec::new();
    let mut basis_keys = Vec::new();
    let mut basis_expansions = Vec::new();
    for (family, items) in &by_family {
        let stat = compute_span_stats(family, items);
        stats.push(stat);
        let classes = find_equiv_classes(family, items, &order_map)?;
        equiv_classes.extend(classes);
        let (rels, search_stat) =
            find_sparse_relations(family, items, cfg.support_max, cfg.coef_set, &order_map)?;
        relations.extend(rels);
        search_stats.push(search_stat);
        if let Some(basis) = compute_basis_mod_p(family, items)? {
            basis_keys.extend(basis.keys);
            basis_expansions.extend(basis.expansions);
        }
    }

    equiv_classes.sort_by(|a, b| compare_equiv_class(&order_map, a, b));
    relations.sort_by(compare_relation);

    let nonzero_observables = total_observables.saturating_sub(trivial_observables);
    if nonzero_observables + trivial_observables != total_observables {
        return Err(ExperimentError::InvalidConfig(
            "nonzero + trivial does not match total".to_string(),
        ));
    }
    let (support_masks, mask_histogram) = compute_support_masks(&by_family)?;
    let allowed_edges = build_allowed_edges(&by_family)?;
    search_stats.sort_by(|a, b| a.family.cmp(&b.family));
    if relations.len() > cfg.top_k {
        relations.truncate(cfg.top_k);
    }

    fs::create_dir_all(&cfg.out_dir)?;
    fs::write(
        cfg.out_dir.join("span_stats.csv"),
        render_span_stats_csv(&stats),
    )?;
    if cfg.export_equiv_classes {
        fs::write(
            cfg.out_dir.join("equiv_classes.csv"),
            render_equiv_classes_csv(&equiv_classes),
        )?;
    }
    fs::write(
        cfg.out_dir.join("span_deps.csv"),
        render_span_deps_csv(&relations),
    )?;
    fs::write(
        cfg.out_dir.join("relations.csv"),
        render_span_deps_csv(&relations),
    )?;
    fs::write(
        cfg.out_dir.join("basis_keys.csv"),
        render_basis_keys_csv(&basis_keys),
    )?;
    fs::write(
        cfg.out_dir.join("basis_expansions_modp.csv"),
        render_basis_expansions_csv(&basis_expansions),
    )?;
    fs::write(
        cfg.out_dir.join("support_mask.csv"),
        render_support_mask_csv(&support_masks),
    )?;
    fs::write(
        cfg.out_dir.join("mask_histogram.csv"),
        render_mask_histogram_csv(&mask_histogram),
    )?;
    fs::write(
        cfg.out_dir.join("allowed_graph.csv"),
        render_allowed_graph_csv(&allowed_edges),
    )?;
    if cfg.export_forbidden {
        let (forbidden, nonzero) = split_keys(&by_family);
        fs::write(
            cfg.out_dir.join("forbidden_keys.csv"),
            render_keys_csv(&forbidden),
        )?;
        fs::write(
            cfg.out_dir.join("nonzero_keys.csv"),
            render_keys_csv(&nonzero),
        )?;
    }
    let md_ctx = SpanDepsMdContext {
        loops: &loops,
        cfg,
        stats: &stats,
        classes: &equiv_classes,
        relations: &relations,
        search_stats: &search_stats,
        basis_keys: &basis_keys,
        mask_histogram: &mask_histogram,
        total_observables,
        trivial_observables,
    };
    fs::write(
        cfg.out_dir.join("span_deps.md"),
        render_span_deps_md(&md_ctx),
    )?;

    Ok(SpanDepsReport {
        loops,
        stats,
        equiv_classes,
        relations,
        search_stats,
        basis_keys,
        mask_histogram,
        total_observables,
        trivial_observables,
    })
}

fn read_observables_csv(path: &Path) -> Result<(Vec<usize>, Vec<Observable>), ExperimentError> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let header_line = match lines.next() {
        Some(line) => line?,
        None => {
            return Err(ExperimentError::InvalidConfig(format!(
                "empty observables CSV: {}",
                path.display()
            )))
        }
    };
    let header = parse_csv_line(&header_line)?;
    let mut family_idx = None;
    let mut params_idx = None;
    let mut loops = Vec::new();
    for (idx, field) in header.iter().enumerate() {
        if field == "family" {
            family_idx = Some(idx);
        } else if field == "params" {
            params_idx = Some(idx);
        } else if let Some(loop_idx) = parse_loop_field(field) {
            loops.push((idx, loop_idx));
        }
    }
    let family_idx = family_idx.ok_or_else(|| {
        ExperimentError::InvalidConfig("observables CSV missing family column".to_string())
    })?;
    let params_idx = params_idx.ok_or_else(|| {
        ExperimentError::InvalidConfig("observables CSV missing params column".to_string())
    })?;
    if loops.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "observables CSV missing cL* columns".to_string(),
        ));
    }
    loops.sort_by_key(|(idx, _)| *idx);
    let loop_values = loops.iter().map(|(_, l)| *l).collect::<Vec<_>>();

    let mut observables = Vec::new();
    let mut row_index = 0usize;
    for line in lines {
        let raw = line?;
        let trimmed = raw.trim_end_matches('\r');
        if trimmed.trim().is_empty() {
            continue;
        }
        let fields = parse_csv_line(trimmed)?;
        if fields.len() < header.len() {
            return Err(ExperimentError::InvalidConfig(format!(
                "observables CSV row has {} columns, expected {}",
                fields.len(),
                header.len()
            )));
        }
        let family = fields[family_idx].clone();
        let params = fields[params_idx].clone();
        let mut values = Vec::with_capacity(loop_values.len());
        for (idx, _) in &loops {
            let value = fields.get(*idx).ok_or_else(|| {
                ExperimentError::InvalidConfig("observables CSV row missing value".to_string())
            })?;
            values.push(parse_coeff(value)?);
        }
        let is_trivial = values.iter().all(|v| v.is_zero());
        let id = format!("{family}|{params}");
        observables.push(Observable {
            family,
            params,
            id,
            order: row_index,
            values,
            is_trivial,
        });
        row_index = row_index.saturating_add(1);
    }

    Ok((loop_values, observables))
}

fn compute_support_masks(
    by_family: &BTreeMap<String, Vec<Observable>>,
) -> Result<(SupportMaskList, MaskHistogramList), ExperimentError> {
    let mut rows = Vec::new();
    let mut histogram: BTreeMap<u64, usize> = BTreeMap::new();
    for items in by_family.values() {
        for obs in items {
            if obs.values.len() > 64 {
                return Err(ExperimentError::InvalidConfig(
                    "support mask requires <= 64 loops".to_string(),
                ));
            }
            let mut mask = 0u64;
            let mut nnz = 0usize;
            for (idx, value) in obs.values.iter().enumerate() {
                if !value.is_zero() {
                    mask |= 1u64 << idx;
                    nnz += 1;
                }
            }
            rows.push(SupportMaskRow {
                key: obs.id.clone(),
                bitmask: mask,
                nnz,
            });
            *histogram.entry(mask).or_insert(0) += 1;
        }
    }
    let mut histogram_rows = Vec::new();
    for (mask, count) in histogram {
        histogram_rows.push(MaskHistogramRow {
            bitmask: mask,
            count,
        });
    }
    Ok((rows, histogram_rows))
}

fn build_allowed_edges(
    by_family: &BTreeMap<String, Vec<Observable>>,
) -> Result<AllowedEdgeList, ExperimentError> {
    let Some(items) = by_family.get("prefix-suffix") else {
        return Ok(Vec::new());
    };
    let mut edges = Vec::new();
    let mut nonzero = 0usize;
    for obs in items {
        if obs.is_trivial {
            continue;
        }
        nonzero += 1;
        let (prefix, suffix) = parse_prefix_suffix_params(&obs.params).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!("invalid prefix-suffix params: {}", obs.params))
        })?;
        edges.push(AllowedEdge { prefix, suffix });
    }
    edges.sort_by(|a, b| {
        let prefix_cmp = a.prefix.cmp(&b.prefix);
        if prefix_cmp != std::cmp::Ordering::Equal {
            return prefix_cmp;
        }
        a.suffix.cmp(&b.suffix)
    });
    if edges.len() != nonzero {
        return Err(ExperimentError::InvalidConfig(
            "allowed graph edge count mismatch".to_string(),
        ));
    }
    Ok(edges)
}

fn parse_prefix_suffix_params(params: &str) -> Option<(String, String)> {
    let mut prefix = None;
    let mut suffix = None;
    for part in params.split(',') {
        let mut iter = part.splitn(2, '=');
        let key = iter.next()?.trim();
        let value = iter.next()?.trim();
        if key == "u" {
            prefix = Some(value.to_string());
        } else if key == "v" {
            suffix = Some(value.to_string());
        }
    }
    Some((prefix?, suffix?))
}

fn parse_loop_field(value: &str) -> Option<usize> {
    let trimmed = value.trim();
    let suffix = trimmed.strip_prefix("cL")?;
    suffix.parse::<usize>().ok()
}

fn parse_csv_line(line: &str) -> Result<Vec<String>, ExperimentError> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if let Some('"') = chars.peek() {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == ',' {
            fields.push(current);
            current = String::new();
        } else {
            current.push(ch);
        }
    }
    if in_quotes {
        return Err(ExperimentError::InvalidConfig(
            "observables CSV has unterminated quote".to_string(),
        ));
    }
    fields.push(current);
    Ok(fields)
}

fn parse_coeff(text: &str) -> Result<Coeff, ExperimentError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "empty coeff in observables CSV".to_string(),
        ));
    }
    if let Some((num, denom)) = trimmed.split_once('/') {
        let n: i64 = num.trim().parse().map_err(|_| {
            ExperimentError::InvalidConfig(format!("invalid coeff numerator: {text}"))
        })?;
        let d: i64 = denom.trim().parse().map_err(|_| {
            ExperimentError::InvalidConfig(format!("invalid coeff denominator: {text}"))
        })?;
        if d == 0 {
            return Err(ExperimentError::InvalidConfig(format!(
                "zero denominator in coeff: {text}"
            )));
        }
        return Ok(Coeff::new(n, d));
    }
    let n: i64 = trimmed
        .parse()
        .map_err(|_| ExperimentError::InvalidConfig(format!("invalid coeff: {text}")))?;
    Ok(Coeff::from_integer(n))
}

fn compute_span_stats(family: &str, items: &[Observable]) -> SpanStats {
    let total = items.len();
    let trivial = items.iter().filter(|obs| obs.is_trivial).count();
    let nonzero = total.saturating_sub(trivial);
    let mut matrix = Vec::new();
    for obs in items {
        if obs.is_trivial {
            continue;
        }
        matrix.push(obs.values.clone());
    }
    let rank = rank_matrix(&matrix);
    let nullity = nonzero.saturating_sub(rank);
    SpanStats {
        family: family.to_string(),
        total,
        nonzero,
        trivial,
        rank,
        nullity,
    }
}

fn rank_matrix(rows: &[Vec<Coeff>]) -> usize {
    if rows.is_empty() {
        return 0;
    }
    let ncols = rows[0].len();
    let mut mat = rows.to_vec();
    let mut rank = 0usize;
    let mut row = 0usize;
    for col in 0..ncols {
        let mut pivot = None;
        for (r, row_vals) in mat.iter().enumerate().skip(row) {
            if !row_vals[col].is_zero() {
                pivot = Some(r);
                break;
            }
        }
        let Some(pivot_row) = pivot else {
            continue;
        };
        mat.swap(row, pivot_row);
        let pivot_val = mat[row][col];
        for value in mat[row].iter_mut().skip(col) {
            *value /= pivot_val;
        }
        let pivot_row = mat[row].clone();
        for (r, row_vals) in mat.iter_mut().enumerate() {
            if r == row {
                continue;
            }
            let factor = row_vals[col];
            if factor.is_zero() {
                continue;
            }
            for (c, value) in row_vals.iter_mut().enumerate().skip(col) {
                *value -= factor * pivot_row[c];
            }
        }
        rank += 1;
        row += 1;
        if row >= mat.len() {
            break;
        }
    }
    rank
}

fn compute_basis_mod_p(
    family: &str,
    items: &[Observable],
) -> Result<Option<BasisModP>, ExperimentError> {
    let nonzero = items
        .iter()
        .filter(|obs| !obs.is_trivial)
        .cloned()
        .collect::<Vec<_>>();
    if nonzero.is_empty() {
        return Ok(None);
    }
    let primes = [1000003, 1000033, 1000037];
    let (prime, mod_vectors) = select_mod_prime(&nonzero, &primes)?;
    let basis_indices = pivot_basis_indices_mod_p(&mod_vectors, prime);
    let mut basis_rows = Vec::new();
    let mut basis_keys = Vec::new();
    for (idx, row_idx) in basis_indices.iter().enumerate() {
        let obs = nonzero.get(*row_idx).ok_or_else(|| {
            ExperimentError::InvalidConfig("basis index out of range".to_string())
        })?;
        basis_rows.push(mod_vectors[*row_idx].clone());
        basis_keys.push(BasisKey {
            family: family.to_string(),
            key: obs.id.clone(),
            index: idx,
        });
    }

    let mut expansions = Vec::new();
    for (idx, obs) in nonzero.iter().enumerate() {
        let coeffs = if basis_rows.is_empty() {
            Vec::new()
        } else {
            solve_coeffs_mod_p(&basis_rows, &mod_vectors[idx], prime).ok_or_else(|| {
                ExperimentError::InvalidConfig(format!(
                    "failed to solve basis expansion for {}",
                    obs.id
                ))
            })?
        };
        expansions.push(BasisExpansion {
            family: family.to_string(),
            key: obs.id.clone(),
            prime,
            coeffs,
        });
    }

    validate_basis_expansions(&basis_rows, &mod_vectors, &expansions, prime, family)?;

    Ok(Some(BasisModP {
        keys: basis_keys,
        expansions,
    }))
}

fn pivot_basis_indices_mod_p(matrix: &[Vec<i64>], p: i64) -> Vec<usize> {
    let mut mat = matrix.to_vec();
    let nrows = mat.len();
    if nrows == 0 {
        return Vec::new();
    }
    let ncols = mat[0].len();
    let mut indices = (0..nrows).collect::<Vec<_>>();
    let mut basis = Vec::new();
    let mut row = 0usize;
    for col in 0..ncols {
        let mut pivot = None;
        for (r, row_vals) in mat.iter().enumerate().skip(row) {
            if mod_i64(row_vals[col], p) != 0 {
                pivot = Some(r);
                break;
            }
        }
        let Some(pivot_row) = pivot else {
            continue;
        };
        mat.swap(row, pivot_row);
        indices.swap(row, pivot_row);
        basis.push(indices[row]);
        let inv = mod_inv(mat[row][col], p);
        for value in mat[row].iter_mut().skip(col) {
            *value = mod_i64(*value * inv, p);
        }
        let pivot_row_vals = mat[row].clone();
        for (r, row_vals) in mat.iter_mut().enumerate() {
            if r == row {
                continue;
            }
            let factor = row_vals[col];
            if factor == 0 {
                continue;
            }
            for (c, value) in row_vals.iter_mut().enumerate().skip(col) {
                *value = mod_i64(*value - factor * pivot_row_vals[c], p);
            }
        }
        row += 1;
        if row >= nrows {
            break;
        }
    }
    basis
}

fn solve_coeffs_mod_p(basis_rows: &[Vec<i64>], target: &[i64], p: i64) -> Option<Vec<i64>> {
    if basis_rows.is_empty() {
        return Some(Vec::new());
    }
    let ncols = basis_rows.len();
    let nrows = target.len();
    let mut mat = vec![vec![0i64; ncols + 1]; nrows];
    for (row_idx, value) in target.iter().enumerate() {
        for (col_idx, basis_row) in basis_rows.iter().enumerate() {
            mat[row_idx][col_idx] = mod_i64(basis_row[row_idx], p);
        }
        mat[row_idx][ncols] = mod_i64(*value, p);
    }

    let mut pivot_rows = vec![None; ncols];
    let mut row = 0usize;
    for col in 0..ncols {
        let mut pivot = None;
        for (r, row_vals) in mat.iter().enumerate().skip(row) {
            if mod_i64(row_vals[col], p) != 0 {
                pivot = Some(r);
                break;
            }
        }
        let Some(pivot_row) = pivot else {
            continue;
        };
        mat.swap(row, pivot_row);
        let inv = mod_inv(mat[row][col], p);
        for value in mat[row].iter_mut().skip(col) {
            *value = mod_i64(*value * inv, p);
        }
        let pivot_row_vals = mat[row].clone();
        for (r, row_vals) in mat.iter_mut().enumerate() {
            if r == row {
                continue;
            }
            let factor = mod_i64(row_vals[col], p);
            if factor == 0 {
                continue;
            }
            for (c, value) in row_vals.iter_mut().enumerate().skip(col) {
                *value = mod_i64(*value - factor * pivot_row_vals[c], p);
            }
        }
        pivot_rows[col] = Some(row);
        row += 1;
        if row >= nrows {
            break;
        }
    }

    for row_vals in &mat {
        let all_zero = row_vals[..ncols]
            .iter()
            .all(|value| mod_i64(*value, p) == 0);
        if all_zero && mod_i64(row_vals[ncols], p) != 0 {
            return None;
        }
    }

    let mut solution = vec![0i64; ncols];
    for (col, pivot_row) in pivot_rows.iter().enumerate() {
        if let Some(row_idx) = pivot_row {
            solution[col] = mod_i64(mat[*row_idx][ncols], p);
        }
    }
    Some(solution)
}

fn validate_basis_expansions(
    basis_rows: &[Vec<i64>],
    mod_vectors: &[Vec<i64>],
    expansions: &[BasisExpansion],
    p: i64,
    family: &str,
) -> Result<(), ExperimentError> {
    if basis_rows.is_empty() {
        return Ok(());
    }
    let sample_count = 5usize.min(expansions.len());
    let indices = sample_indices(expansions.len(), sample_count);
    for idx in indices {
        let coeffs = &expansions[idx].coeffs;
        let target = &mod_vectors[idx];
        if !verify_expansion_mod_p(basis_rows, coeffs, target, p) {
            return Err(ExperimentError::InvalidConfig(format!(
                "basis expansion verification failed for family {family}"
            )));
        }
    }
    Ok(())
}

fn verify_expansion_mod_p(basis_rows: &[Vec<i64>], coeffs: &[i64], target: &[i64], p: i64) -> bool {
    if basis_rows.is_empty() {
        return target.iter().all(|value| mod_i64(*value, p) == 0);
    }
    let ncols = target.len();
    for col in 0..ncols {
        let mut sum = 0i64;
        for (coeff, basis_row) in coeffs.iter().zip(basis_rows.iter()) {
            sum = mod_i64(sum + coeff * basis_row[col], p);
        }
        if mod_i64(sum, p) != mod_i64(target[col], p) {
            return false;
        }
    }
    true
}

fn sample_indices(total: usize, count: usize) -> Vec<usize> {
    if total == 0 || count == 0 {
        return Vec::new();
    }
    if total <= count {
        return (0..total).collect();
    }
    let mut out = BTreeSet::new();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    while out.len() < count {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        out.insert((state % total as u64) as usize);
    }
    out.into_iter().collect()
}

fn find_equiv_classes(
    family: &str,
    items: &[Observable],
    order_map: &BTreeMap<String, usize>,
) -> Result<Vec<EquivClass>, ExperimentError> {
    let mut groups: EquivGroups = BTreeMap::new();
    for obs in items {
        if obs.is_trivial {
            continue;
        }
        let Some((norm, scale)) = normalize_vector(&obs.values) else {
            continue;
        };
        groups
            .entry(norm)
            .or_default()
            .push((obs.id.clone(), scale, obs.values.clone()));
    }
    let mut out = Vec::new();
    for (_, mut members) in groups {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|a, b| compare_id(order_map, &a.0, &b.0));
        let rep_id = members[0].0.clone();
        let rep_scale = members[0].1;
        let rep_values = members[0].2.clone();
        let mut class_members = Vec::new();
        for (id, scale, values) in members {
            let ratio = scale / rep_scale;
            if !values_match_ratio(&values, &rep_values, &ratio) {
                return Err(ExperimentError::InvalidConfig(format!(
                    "equiv class ratio mismatch for {id}"
                )));
            }
            class_members.push(EquivMember { id, ratio });
        }
        out.push(EquivClass {
            family: family.to_string(),
            representative: rep_id,
            members: class_members,
        });
    }
    Ok(out)
}

fn normalize_vector(values: &[Coeff]) -> Option<(Vec<Coeff>, Coeff)> {
    let mut first = None;
    for value in values {
        if !value.is_zero() {
            first = Some(*value);
            break;
        }
    }
    let scale = first?;
    let normalized = values.iter().map(|v| *v / scale).collect::<Vec<_>>();
    Some((normalized, scale))
}

fn values_match_ratio(values: &[Coeff], rep: &[Coeff], ratio: &Coeff) -> bool {
    values
        .iter()
        .zip(rep.iter())
        .all(|(v, r)| *v == *ratio * *r)
}

fn find_sparse_relations(
    family: &str,
    items: &[Observable],
    support_max: usize,
    coef_set: CoefSet,
    order_map: &BTreeMap<String, usize>,
) -> Result<(Vec<SparseRelation>, SparseSearchStats), ExperimentError> {
    let mut relations = BTreeMap::new();
    let mut nonzero = Vec::new();
    for obs in items {
        if obs.is_trivial {
            continue;
        }
        nonzero.push(obs.clone());
    }
    nonzero.sort_by(|a, b| compare_observable(order_map, a, b));
    let mut value_map: BTreeMap<Vec<Coeff>, Vec<usize>> = BTreeMap::new();
    for (idx, obs) in nonzero.iter().enumerate() {
        value_map.entry(obs.values.clone()).or_default().push(idx);
    }

    let equiv_classes = find_equiv_classes(family, &nonzero, order_map)?;
    for class in &equiv_classes {
        if class.members.len() < 2 {
            continue;
        }
        let rep = &class.members[0];
        for member in class.members.iter().skip(1) {
            if coef_set == CoefSet::Pm1 {
                if member.ratio == Coeff::from_integer(1) || member.ratio == Coeff::from_integer(-1)
                {
                    let coeff_rep = if member.ratio == Coeff::from_integer(1) {
                        -1
                    } else {
                        1
                    };
                    let terms = vec![(1, member.id.clone()), (coeff_rep, rep.id.clone())];
                    insert_relation(&mut relations, family, terms, order_map);
                }
            } else {
                let ratio = member.ratio;
                let coeffs = ratio_to_coeffs_pm2(&ratio)?;
                if let Some((a, b)) = coeffs {
                    let terms = vec![(a, member.id.clone()), (b, rep.id.clone())];
                    insert_relation(&mut relations, family, terms, order_map);
                }
            }
        }
    }

    if support_max >= 3 && coef_set == CoefSet::Pm1 {
        for i in 0..nonzero.len() {
            for j in (i + 1)..nonzero.len() {
                let sum = add_vectors(&nonzero[i].values, &nonzero[j].values);
                if let Some(indices) = value_map.get(&sum) {
                    for &k in indices {
                        if k == i || k == j {
                            continue;
                        }
                        let terms = vec![
                            (1, nonzero[i].id.clone()),
                            (1, nonzero[j].id.clone()),
                            (-1, nonzero[k].id.clone()),
                        ];
                        insert_relation(&mut relations, family, terms, order_map);
                    }
                }
                let diff = sub_vectors(&nonzero[i].values, &nonzero[j].values);
                if let Some(indices) = value_map.get(&diff) {
                    for &k in indices {
                        if k == i || k == j {
                            continue;
                        }
                        let terms = vec![
                            (1, nonzero[i].id.clone()),
                            (-1, nonzero[j].id.clone()),
                            (-1, nonzero[k].id.clone()),
                        ];
                        insert_relation(&mut relations, family, terms, order_map);
                    }
                }
            }
        }
    }

    let mut stats = SparseSearchStats {
        family: family.to_string(),
        attempts: 0,
        hits_mod_p: 0,
        verified: 0,
    };
    if support_max >= 3 && coef_set == CoefSet::Pm2 {
        let search = pm2_support3_search(&nonzero, order_map)?;
        stats.attempts = search.attempts;
        stats.hits_mod_p = search.hits_mod_p;
        stats.verified = search.verified;
        for relation in search.relations {
            insert_relation(&mut relations, family, relation, order_map);
        }
    }
    if support_max >= 4 && coef_set == CoefSet::Pm2 {
        let search = pm2_support4_search(&nonzero, order_map)?;
        stats.attempts += search.attempts;
        stats.hits_mod_p += search.hits_mod_p;
        stats.verified += search.verified;
        for relation in search.relations {
            insert_relation(&mut relations, family, relation, order_map);
        }
    }

    Ok((relations.into_values().collect(), stats))
}

fn ratio_to_coeffs_pm2(ratio: &Coeff) -> Result<Option<(i32, i32)>, ExperimentError> {
    let numer = *ratio.numer();
    let denom = *ratio.denom();
    let candidates = [
        (1, -1),
        (1, 1),
        (1, -2),
        (1, 2),
        (2, -1),
        (2, 1),
        (2, -2),
        (2, 2),
    ];
    for (a, b) in candidates {
        let q = Coeff::new(-(b as i64), a as i64);
        if q.numer() == &numer && q.denom() == &denom {
            return Ok(Some((a, b)));
        }
    }
    Ok(None)
}

fn insert_relation(
    relations: &mut BTreeMap<String, SparseRelation>,
    family: &str,
    terms: Vec<(i32, String)>,
    order_map: &BTreeMap<String, usize>,
) {
    let normalized = normalize_terms(terms, order_map);
    if normalized.is_empty() {
        return;
    }
    let key = relation_key(&normalized);
    relations.entry(key).or_insert(SparseRelation {
        family: family.to_string(),
        terms: normalized,
    });
}

fn normalize_terms(
    mut terms: Vec<(i32, String)>,
    order_map: &BTreeMap<String, usize>,
) -> Vec<(i32, String)> {
    terms.retain(|(c, _)| *c != 0);
    terms.sort_by(|a, b| compare_id(order_map, &a.1, &b.1));
    if let Some(first) = terms.first_mut() {
        if first.0 < 0 {
            for term in &mut terms {
                term.0 = -term.0;
            }
        }
    }
    terms
}

fn relation_key(terms: &[(i32, String)]) -> String {
    let mut out = String::new();
    for (idx, (coeff, id)) in terms.iter().enumerate() {
        if idx > 0 {
            out.push(';');
        }
        out.push_str(&format!("{coeff}*{id}"));
    }
    out
}

fn add_vectors(a: &[Coeff], b: &[Coeff]) -> Vec<Coeff> {
    a.iter().zip(b.iter()).map(|(x, y)| *x + *y).collect()
}

fn sub_vectors(a: &[Coeff], b: &[Coeff]) -> Vec<Coeff> {
    a.iter().zip(b.iter()).map(|(x, y)| *x - *y).collect()
}

struct Pm2SearchResult {
    relations: Vec<Vec<(i32, String)>>,
    attempts: usize,
    hits_mod_p: usize,
    verified: usize,
}

fn pm2_support3_search(
    items: &[Observable],
    order_map: &BTreeMap<String, usize>,
) -> Result<Pm2SearchResult, ExperimentError> {
    if items.len() < 3 {
        return Ok(Pm2SearchResult {
            relations: Vec::new(),
            attempts: 0,
            hits_mod_p: 0,
            verified: 0,
        });
    }
    let primes = [1000003, 1000033, 1000037];
    let (prime, mod_vectors) = select_mod_prime(items, &primes)?;
    let mut mod_map: BTreeMap<Vec<i64>, Vec<usize>> = BTreeMap::new();
    for (idx, vec) in mod_vectors.iter().enumerate() {
        mod_map.entry(vec.clone()).or_default().push(idx);
    }

    let coeffs = [-2, -1, 1, 2];
    let mut templates = Vec::with_capacity(coeffs.len() * coeffs.len());
    for &a in &coeffs {
        for &b in &coeffs {
            templates.push((a, b, -1));
        }
    }

    let mut attempts = 0usize;
    let mut hits = 0usize;
    let mut verified = 0usize;
    let mut relations = Vec::new();
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            let vi_mod = &mod_vectors[i];
            let vj_mod = &mod_vectors[j];
            for (a, b, c) in &templates {
                attempts += 1;
                let target = combine_target_mod(vi_mod, vj_mod, *a, *b, *c, prime);
                let Some(indices) = mod_map.get(&target) else {
                    continue;
                };
                for &k in indices {
                    if k == i || k == j {
                        continue;
                    }
                    hits += 1;
                    if !verify_relation(
                        &items[i].values,
                        &items[j].values,
                        &items[k].values,
                        *a,
                        *b,
                        *c,
                    ) {
                        continue;
                    }
                    verified += 1;
                    let terms = vec![
                        (*a, items[i].id.clone()),
                        (*b, items[j].id.clone()),
                        (*c, items[k].id.clone()),
                    ];
                    relations.push(terms);
                }
            }
        }
    }

    relations.sort_by(|left, right| {
        let key_left = normalize_terms(left.clone(), order_map);
        let key_right = normalize_terms(right.clone(), order_map);
        relation_key(&key_left).cmp(&relation_key(&key_right))
    });
    Ok(Pm2SearchResult {
        relations,
        attempts,
        hits_mod_p: hits,
        verified,
    })
}

fn combine_target_mod(left: &[i64], right: &[i64], a: i32, b: i32, c: i32, p: i64) -> Vec<i64> {
    let mut out = Vec::with_capacity(left.len());
    let sign = if c == 1 { -1 } else { 1 };
    let a = a as i64;
    let b = b as i64;
    for (x, y) in left.iter().zip(right.iter()) {
        let value = mod_i64(sign as i64 * (a * *x + b * *y), p);
        out.push(value);
    }
    out
}

fn verify_relation(a: &[Coeff], b: &[Coeff], c: &[Coeff], ca: i32, cb: i32, cc: i32) -> bool {
    let fa = Coeff::from_integer(ca as i64);
    let fb = Coeff::from_integer(cb as i64);
    let fc = Coeff::from_integer(cc as i64);
    a.iter()
        .zip(b.iter())
        .zip(c.iter())
        .all(|((x, y), z)| *x * fa + *y * fb + *z * fc == Coeff::zero())
}

fn pm2_support4_search(
    items: &[Observable],
    order_map: &BTreeMap<String, usize>,
) -> Result<Pm2SearchResult, ExperimentError> {
    if items.len() < 4 {
        return Ok(Pm2SearchResult {
            relations: Vec::new(),
            attempts: 0,
            hits_mod_p: 0,
            verified: 0,
        });
    }
    let primes = [1000003, 1000033, 1000037];
    let (prime, mod_vectors) = select_mod_prime(items, &primes)?;
    let mut mod_map: BTreeMap<Vec<i64>, Vec<usize>> = BTreeMap::new();
    for (idx, vec) in mod_vectors.iter().enumerate() {
        mod_map.entry(vec.clone()).or_default().push(idx);
    }

    let coeffs = [-2, -1, 1, 2];
    let mut templates = Vec::with_capacity(coeffs.len() * coeffs.len() * coeffs.len());
    for &a in &coeffs {
        for &b in &coeffs {
            for &c in &coeffs {
                templates.push((a, b, c));
            }
        }
    }

    let mut attempts = 0usize;
    let mut hits = 0usize;
    let mut verified = 0usize;
    let mut relations = Vec::new();
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            for k in (j + 1)..items.len() {
                let vi_mod = &mod_vectors[i];
                let vj_mod = &mod_vectors[j];
                let vk_mod = &mod_vectors[k];
                for (a, b, c) in &templates {
                    attempts += 1;
                    let target = combine_target_mod3(vi_mod, vj_mod, vk_mod, *a, *b, *c, prime);
                    let Some(indices) = mod_map.get(&target) else {
                        continue;
                    };
                    for &l in indices {
                        if l == i || l == j || l == k {
                            continue;
                        }
                        hits += 1;
                        if !verify_relation4(
                            [
                                &items[i].values,
                                &items[j].values,
                                &items[k].values,
                                &items[l].values,
                            ],
                            [*a, *b, *c, -1],
                        ) {
                            continue;
                        }
                        verified += 1;
                        let terms = vec![
                            (*a, items[i].id.clone()),
                            (*b, items[j].id.clone()),
                            (*c, items[k].id.clone()),
                            (-1, items[l].id.clone()),
                        ];
                        relations.push(terms);
                    }
                }
            }
        }
    }

    relations.sort_by(|left, right| {
        let key_left = normalize_terms(left.clone(), order_map);
        let key_right = normalize_terms(right.clone(), order_map);
        relation_key(&key_left).cmp(&relation_key(&key_right))
    });
    Ok(Pm2SearchResult {
        relations,
        attempts,
        hits_mod_p: hits,
        verified,
    })
}

fn combine_target_mod3(
    first: &[i64],
    second: &[i64],
    third: &[i64],
    a: i32,
    b: i32,
    c: i32,
    p: i64,
) -> Vec<i64> {
    let mut out = Vec::with_capacity(first.len());
    let a = a as i64;
    let b = b as i64;
    let c = c as i64;
    for ((x, y), z) in first.iter().zip(second.iter()).zip(third.iter()) {
        let value = mod_i64(a * *x + b * *y + c * *z, p);
        out.push(value);
    }
    out
}

fn verify_relation4(values: [&[Coeff]; 4], coeffs: [i32; 4]) -> bool {
    let fa = Coeff::from_integer(coeffs[0] as i64);
    let fb = Coeff::from_integer(coeffs[1] as i64);
    let fc = Coeff::from_integer(coeffs[2] as i64);
    let fd = Coeff::from_integer(coeffs[3] as i64);
    values[0]
        .iter()
        .zip(values[1].iter())
        .zip(values[2].iter())
        .zip(values[3].iter())
        .all(|(((x, y), z), w)| *x * fa + *y * fb + *z * fc + *w * fd == Coeff::zero())
}

fn select_mod_prime(
    items: &[Observable],
    primes: &[i64],
) -> Result<(i64, Vec<Vec<i64>>), ExperimentError> {
    let mut best_rank = None;
    let mut best_prime = 0i64;
    let mut best_vectors = None;
    for &prime in primes {
        let Some(mod_vectors) = values_to_mod(items, prime) else {
            continue;
        };
        let rank = rank_mod_p_i64(&mod_vectors, prime);
        let replace = match best_rank {
            None => true,
            Some(current) => rank > current || (rank == current && prime < best_prime),
        };
        if replace {
            best_rank = Some(rank);
            best_prime = prime;
            best_vectors = Some(mod_vectors);
        }
    }
    let Some(rank) = best_rank else {
        return Err(ExperimentError::InvalidConfig(
            "no usable primes for mod-p search".to_string(),
        ));
    };
    let _ = rank;
    Ok((best_prime, best_vectors.unwrap_or_default()))
}

fn values_to_mod(items: &[Observable], p: i64) -> Option<Vec<Vec<i64>>> {
    let mut out = Vec::with_capacity(items.len());
    for obs in items {
        let mut row = Vec::with_capacity(obs.values.len());
        for coeff in &obs.values {
            let denom = *coeff.denom();
            let denom_mod = mod_i64(denom, p);
            if denom_mod == 0 {
                return None;
            }
            let inv = mod_inv(denom_mod, p);
            let numer = mod_i64(*coeff.numer(), p);
            row.push(mod_i64(numer * inv, p));
        }
        out.push(row);
    }
    Some(out)
}

fn rank_mod_p_i64(matrix: &[Vec<i64>], p: i64) -> usize {
    let mut mat = matrix.to_vec();
    let nrows = mat.len();
    if nrows == 0 {
        return 0;
    }
    let ncols = mat[0].len();
    let mut rank = 0usize;
    let mut row = 0usize;
    for col in 0..ncols {
        let mut pivot = None;
        for (r, row_vals) in mat.iter().enumerate().skip(row) {
            if mod_i64(row_vals[col], p) != 0 {
                pivot = Some(r);
                break;
            }
        }
        let Some(pivot_row) = pivot else {
            continue;
        };
        mat.swap(row, pivot_row);
        let inv = mod_inv(mat[row][col], p);
        for value in mat[row].iter_mut().skip(col) {
            *value = mod_i64(*value * inv, p);
        }
        let pivot_row = mat[row].clone();
        for (r, row_vals) in mat.iter_mut().enumerate() {
            if r == row {
                continue;
            }
            let factor = row_vals[col];
            if factor == 0 {
                continue;
            }
            for (c, value) in row_vals.iter_mut().enumerate().skip(col) {
                *value = mod_i64(*value - factor * pivot_row[c], p);
            }
        }
        rank += 1;
        row += 1;
        if row >= nrows {
            break;
        }
    }
    rank
}

fn mod_i64(value: i64, p: i64) -> i64 {
    let mut v = value % p;
    if v < 0 {
        v += p;
    }
    v
}

fn mod_inv(value: i64, p: i64) -> i64 {
    mod_pow(mod_i64(value, p), p - 2, p)
}

fn mod_pow(mut base: i64, mut exp: i64, p: i64) -> i64 {
    let mut out = 1i64;
    base = mod_i64(base, p);
    while exp > 0 {
        if exp & 1 == 1 {
            out = mod_i64(out * base, p);
        }
        base = mod_i64(base * base, p);
        exp >>= 1;
    }
    out
}

fn compare_relation(a: &SparseRelation, b: &SparseRelation) -> std::cmp::Ordering {
    let fam = a.family.cmp(&b.family);
    if fam != std::cmp::Ordering::Equal {
        return fam;
    }
    let support_cmp = a.terms.len().cmp(&b.terms.len());
    if support_cmp != std::cmp::Ordering::Equal {
        return support_cmp;
    }
    let key_a = relation_key(&a.terms);
    let key_b = relation_key(&b.terms);
    key_a.cmp(&key_b)
}

fn build_order_map(observables: &[Observable]) -> Result<BTreeMap<String, usize>, ExperimentError> {
    let mut map = BTreeMap::new();
    for obs in observables {
        if let Some(existing) = map.insert(obs.id.clone(), obs.order) {
            if existing != obs.order {
                return Err(ExperimentError::InvalidConfig(format!(
                    "duplicate observable key: {}",
                    obs.id
                )));
            }
        }
    }
    Ok(map)
}

fn compare_observable(
    order_map: &BTreeMap<String, usize>,
    a: &Observable,
    b: &Observable,
) -> std::cmp::Ordering {
    compare_id(order_map, &a.id, &b.id)
}

fn compare_id(order_map: &BTreeMap<String, usize>, left: &str, right: &str) -> std::cmp::Ordering {
    let left_order = order_map.get(left).copied().unwrap_or(usize::MAX);
    let right_order = order_map.get(right).copied().unwrap_or(usize::MAX);
    let order_cmp = left_order.cmp(&right_order);
    if order_cmp != std::cmp::Ordering::Equal {
        return order_cmp;
    }
    left.cmp(right)
}

fn compare_equiv_class(
    order_map: &BTreeMap<String, usize>,
    a: &EquivClass,
    b: &EquivClass,
) -> std::cmp::Ordering {
    let size_cmp = b.members.len().cmp(&a.members.len());
    if size_cmp != std::cmp::Ordering::Equal {
        return size_cmp;
    }
    let fam_cmp = a.family.cmp(&b.family);
    if fam_cmp != std::cmp::Ordering::Equal {
        return fam_cmp;
    }
    compare_id(order_map, &a.representative, &b.representative)
}

fn split_keys(by_family: &BTreeMap<String, Vec<Observable>>) -> (KeyList, KeyList) {
    let mut forbidden = Vec::new();
    let mut nonzero = Vec::new();
    for (family, items) in by_family {
        for obs in items {
            if obs.is_trivial {
                forbidden.push((family.clone(), obs.params.clone()));
            } else {
                nonzero.push((family.clone(), obs.params.clone()));
            }
        }
    }
    (forbidden, nonzero)
}

fn render_keys_csv(rows: &[(String, String)]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["family", "params"]);
    for (family, params) in rows {
        writer.push_record([family.clone(), params.clone()]);
    }
    writer.into_string()
}

fn render_span_stats_csv(stats: &[SpanStats]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["family", "total", "nonzero", "trivial", "rank", "nullity"]);
    let mut rows = stats.to_vec();
    rows.sort_by(|a, b| a.family.cmp(&b.family));
    for stat in rows {
        writer.push_record([
            stat.family,
            stat.total.to_string(),
            stat.nonzero.to_string(),
            stat.trivial.to_string(),
            stat.rank.to_string(),
            stat.nullity.to_string(),
        ]);
    }
    writer.into_string()
}

fn render_equiv_classes_csv(classes: &[EquivClass]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["family", "representative", "member", "ratio"]);
    for class in classes {
        for member in &class.members {
            writer.push_record([
                class.family.clone(),
                class.representative.clone(),
                member.id.clone(),
                format_coeff(&member.ratio),
            ]);
        }
    }
    writer.into_string()
}

fn render_span_deps_csv(relations: &[SparseRelation]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["family", "support", "term1", "term2", "term3", "term4"]);
    for rel in relations {
        let mut terms = rel
            .terms
            .iter()
            .map(|(coeff, id)| format!("{coeff}*{id}"))
            .collect::<Vec<_>>();
        while terms.len() < 4 {
            terms.push(String::new());
        }
        writer.push_record([
            rel.family.clone(),
            rel.terms.len().to_string(),
            terms[0].clone(),
            terms[1].clone(),
            terms[2].clone(),
            terms[3].clone(),
        ]);
    }
    writer.into_string()
}

fn render_basis_keys_csv(keys: &[BasisKey]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["family", "basis_index", "key"]);
    for row in keys {
        writer.push_record([row.family.clone(), row.index.to_string(), row.key.clone()]);
    }
    writer.into_string()
}

fn render_basis_expansions_csv(expansions: &[BasisExpansion]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["family", "key", "prime", "coeffs"]);
    for row in expansions {
        let coeffs = row
            .coeffs
            .iter()
            .map(|coeff| coeff.to_string())
            .collect::<Vec<_>>()
            .join(";");
        writer.push_record([
            row.family.clone(),
            row.key.clone(),
            row.prime.to_string(),
            coeffs,
        ]);
    }
    writer.into_string()
}

fn render_support_mask_csv(rows: &[SupportMaskRow]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["key", "bitmask", "nnz_count"]);
    for row in rows {
        writer.push_record([
            row.key.clone(),
            row.bitmask.to_string(),
            row.nnz.to_string(),
        ]);
    }
    writer.into_string()
}

fn render_mask_histogram_csv(rows: &[MaskHistogramRow]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["bitmask", "count"]);
    for row in rows {
        writer.push_record([row.bitmask.to_string(), row.count.to_string()]);
    }
    writer.into_string()
}

fn render_allowed_graph_csv(edges: &[AllowedEdge]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["prefix_id", "suffix_id"]);
    for edge in edges {
        writer.push_record([edge.prefix.clone(), edge.suffix.clone()]);
    }
    writer.into_string()
}

struct SpanDepsMdContext<'a> {
    loops: &'a [usize],
    cfg: &'a EsymbSpanDepsConfig,
    stats: &'a [SpanStats],
    classes: &'a [EquivClass],
    relations: &'a [SparseRelation],
    search_stats: &'a [SparseSearchStats],
    basis_keys: &'a [BasisKey],
    mask_histogram: &'a [MaskHistogramRow],
    total_observables: usize,
    trivial_observables: usize,
}

fn render_span_deps_md(ctx: &SpanDepsMdContext) -> String {
    let mut out = String::new();
    out.push_str("# esymb_span_deps\n\n");
    out.push_str(&format!("loops = {}\n\n", format_usize_list(ctx.loops)));
    out.push_str(&format!("family = {}\n\n", ctx.cfg.family.as_str()));
    out.push_str(&format!("support_max = {}\n\n", ctx.cfg.support_max));
    out.push_str(&format!("coef_set = {}\n\n", ctx.cfg.coef_set.as_str()));
    out.push_str(&format!("top_k = {}\n\n", ctx.cfg.top_k));
    out.push_str(&format!(
        "observables_total = {}\n\n",
        ctx.total_observables
    ));
    out.push_str(&format!(
        "observables_trivial = {}\n\n",
        ctx.trivial_observables
    ));

    out.push_str("## family_stats\n\n");
    if ctx.stats.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        out.push_str("| family | total | nonzero | trivial | rank | nullity |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- |\n");
        let mut rows = ctx.stats.to_vec();
        rows.sort_by(|a, b| a.family.cmp(&b.family));
        for stat in rows {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                stat.family, stat.total, stat.nonzero, stat.trivial, stat.rank, stat.nullity
            ));
        }
        out.push('\n');
    }

    out.push_str("## density\n\n");
    let overall_nonzero = ctx
        .total_observables
        .saturating_sub(ctx.trivial_observables);
    out.push_str(&format!(
        "overall = {}/{} ({})\n\n",
        overall_nonzero,
        ctx.total_observables,
        format_density(overall_nonzero, ctx.total_observables)
    ));
    if !ctx.stats.is_empty() {
        let mut rows = ctx.stats.to_vec();
        rows.sort_by(|a, b| a.family.cmp(&b.family));
        for stat in rows {
            out.push_str(&format!(
                "- {}: {}/{} ({})\n",
                stat.family,
                stat.nonzero,
                stat.total,
                format_density(stat.nonzero, stat.total)
            ));
        }
        out.push('\n');
    }

    out.push_str("## top_equiv_classes\n\n");
    if ctx.classes.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        let limit = ctx.cfg.top_k.min(ctx.classes.len());
        for class in ctx.classes.iter().take(limit) {
            let members_str = class
                .members
                .iter()
                .map(|m| format!("{}:{}", m.id, format_coeff(&m.ratio)))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "- family={} rep={} size={} members=[{}]\n",
                class.family,
                class.representative,
                class.members.len(),
                members_str
            ));
        }
        out.push('\n');
    }

    out.push_str("## sample_relations\n\n");
    if ctx.relations.is_empty() {
        out.push_str("_none_\n");
    } else {
        let limit = 20usize.min(ctx.relations.len());
        for rel in ctx.relations.iter().take(limit) {
            let terms = rel
                .terms
                .iter()
                .map(|(coeff, id)| format!("{coeff}*{id}"))
                .collect::<Vec<_>>()
                .join(" + ");
            out.push_str(&format!("- {}: {} = 0\n", rel.family, terms));
        }
    }

    out.push_str("\n## basis_keys\n\n");
    if ctx.basis_keys.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        out.push_str(&format!("count = {}\n\n", ctx.basis_keys.len()));
        let limit = 20usize.min(ctx.basis_keys.len());
        for row in ctx.basis_keys.iter().take(limit) {
            out.push_str(&format!("- {}: {}\n", row.family, row.key));
        }
        out.push('\n');
    }

    out.push_str("## mask_histogram_summary\n\n");
    if ctx.mask_histogram.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        let limit = 10usize.min(ctx.mask_histogram.len());
        for row in ctx.mask_histogram.iter().take(limit) {
            out.push_str(&format!("- mask {}: {}\n", row.bitmask, row.count));
        }
        out.push('\n');
    }

    out.push_str("## sparse_search_stats\n\n");
    if ctx.search_stats.is_empty() {
        out.push_str("_none_\n");
        return out;
    }
    for stat in ctx.search_stats {
        out.push_str(&format!(
            "- family={} attempts={} hits_mod_p={} verified={}\n",
            stat.family, stat.attempts, stat.hits_mod_p, stat.verified
        ));
    }
    out
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

fn format_density(nonzero: usize, total: usize) -> String {
    if total == 0 {
        return "0.000000".to_string();
    }
    let density = nonzero as f64 / total as f64;
    format!("{density:.6}")
}

fn format_usize_list(values: &[usize]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    let rendered = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rendered}]")
}
