use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use mpl_symbol::Coeff;
use num_traits::Zero;

use crate::output::csv::CsvWriter;
use crate::ExperimentError;

#[derive(Clone, Debug)]
pub struct EsymbHankelSubblockConfig {
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub r: usize,
    pub k: usize,
    pub loops: Option<Vec<usize>>,
    pub primes: Vec<i64>,
    pub exact: bool,
}

#[derive(Clone, Debug)]
pub struct HankelSubblockStats {
    pub loop_index: usize,
    pub nrows: usize,
    pub ncols: usize,
    pub rank_mod_p: usize,
}

#[derive(Clone, Debug)]
pub struct HankelDependency {
    pub loop_index: usize,
    pub prime: i64,
    pub id: String,
    pub terms: Vec<(i64, String)>,
}

#[derive(Clone, Debug)]
pub struct HankelSubblockReport {
    pub loops: Vec<usize>,
    pub stats: Vec<HankelSubblockStats>,
    pub row_deps: Vec<HankelDependency>,
    pub col_deps: Vec<HankelDependency>,
    pub nrows: usize,
    pub ncols: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PairKey {
    u: String,
    v: String,
}

#[derive(Clone, Debug)]
struct ObservableRow {
    u: String,
    v: String,
    values: Vec<Coeff>,
}

pub fn run_esymb_hankel_subblock(
    cfg: &EsymbHankelSubblockConfig,
) -> Result<HankelSubblockReport, ExperimentError> {
    if cfg.primes.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "--primes must not be empty".to_string(),
        ));
    }

    let (csv_loops, rows, u_order, v_order) = read_prefix_suffix_rows(&cfg.input, cfg.r, cfg.k)?;
    if u_order.is_empty() || v_order.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "no prefix-suffix rows for requested r,k".to_string(),
        ));
    }

    let loops = if let Some(list) = cfg.loops.clone() {
        let mut loop_index = BTreeMap::new();
        for (idx, value) in csv_loops.iter().enumerate() {
            loop_index.insert(*value, idx);
        }
        for value in &list {
            if !loop_index.contains_key(value) {
                return Err(ExperimentError::InvalidConfig(format!(
                    "loop L={value} not found in observables CSV"
                )));
            }
        }
        list
    } else {
        csv_loops.clone()
    };

    let mut loop_index = BTreeMap::new();
    for (idx, value) in csv_loops.iter().enumerate() {
        loop_index.insert(*value, idx);
    }
    let loop_indices = loops
        .iter()
        .map(|value| loop_index[value])
        .collect::<Vec<_>>();

    let mut values_map: BTreeMap<PairKey, Vec<Coeff>> = BTreeMap::new();
    for row in rows {
        let mut selected = Vec::with_capacity(loop_indices.len());
        for idx in &loop_indices {
            selected.push(row.values[*idx]);
        }
        let key = PairKey {
            u: row.u.clone(),
            v: row.v.clone(),
        };
        values_map
            .entry(key)
            .and_modify(|vals| {
                for (slot, value) in vals.iter_mut().zip(selected.iter()) {
                    *slot += *value;
                }
            })
            .or_insert(selected);
    }

    let nrows = u_order.len();
    let ncols = v_order.len();
    let mut stats = Vec::new();
    let mut row_deps = Vec::new();
    let mut col_deps = Vec::new();
    for (loop_idx, loop_value) in loops.iter().copied().enumerate() {
        let matrix = build_matrix(&u_order, &v_order, &values_map, loop_idx);
        let (rank_mod_p, prime_for_deps, mod_matrix) = rank_and_prime(&matrix, &cfg.primes)?;
        stats.push(HankelSubblockStats {
            loop_index: loop_value,
            nrows,
            ncols,
            rank_mod_p,
        });

        if cfg.exact {
            let row_ids = u_order.iter().map(|u| format!("u={u}")).collect::<Vec<_>>();
            let deps =
                compute_dependencies_mod_p(&mod_matrix, &row_ids, loop_value, prime_for_deps)?;
            row_deps.extend(deps);

            let col_ids = v_order.iter().map(|v| format!("v={v}")).collect::<Vec<_>>();
            let col_matrix = transpose_mod_p(&mod_matrix);
            let deps =
                compute_dependencies_mod_p(&col_matrix, &col_ids, loop_value, prime_for_deps)?;
            col_deps.extend(deps);
        }
    }

    row_deps.sort_by(compare_dependency);
    col_deps.sort_by(compare_dependency);
    stats.sort_by(|a, b| a.loop_index.cmp(&b.loop_index));

    fs::create_dir_all(&cfg.out_dir)?;
    fs::write(
        cfg.out_dir.join("hankel_subblock_stats.csv"),
        render_stats_csv(cfg, &stats),
    )?;
    if cfg.exact {
        fs::write(
            cfg.out_dir.join("hankel_row_deps.csv"),
            render_deps_csv(cfg, &row_deps, "row"),
        )?;
        fs::write(
            cfg.out_dir.join("hankel_col_deps.csv"),
            render_deps_csv(cfg, &col_deps, "col"),
        )?;
    }
    fs::write(
        cfg.out_dir.join("hankel_subblock.md"),
        render_md(cfg, &loops, &stats, &row_deps, &col_deps),
    )?;

    Ok(HankelSubblockReport {
        loops,
        stats,
        row_deps,
        col_deps,
        nrows,
        ncols,
    })
}

type PrefixSuffixRows = (Vec<usize>, Vec<ObservableRow>, Vec<String>, Vec<String>);

fn read_prefix_suffix_rows(
    path: &Path,
    target_r: usize,
    target_k: usize,
) -> Result<PrefixSuffixRows, ExperimentError> {
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

    let mut rows = Vec::new();
    let mut seen_u = BTreeSet::new();
    let mut seen_v = BTreeSet::new();
    let mut u_order = Vec::new();
    let mut v_order = Vec::new();
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
        let family = fields[family_idx].trim();
        if family != "prefix-suffix" {
            continue;
        }
        let params = fields[params_idx].trim();
        let (r, k, u, v) = parse_params(params)?;
        if r != target_r || k != target_k {
            continue;
        }
        let mut values = Vec::with_capacity(loop_values.len());
        for (idx, _) in &loops {
            let value = fields.get(*idx).ok_or_else(|| {
                ExperimentError::InvalidConfig("observables CSV row missing value".to_string())
            })?;
            values.push(parse_coeff(value)?);
        }
        if seen_u.insert(u.clone()) {
            u_order.push(u.clone());
        }
        if seen_v.insert(v.clone()) {
            v_order.push(v.clone());
        }
        rows.push(ObservableRow { u, v, values });
    }

    Ok((loop_values, rows, u_order, v_order))
}

fn parse_params(params: &str) -> Result<(usize, usize, String, String), ExperimentError> {
    let mut r = None;
    let mut k = None;
    let mut u = None;
    let mut v = None;
    for part in params.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "r" => {
                let parsed = value.parse::<usize>().map_err(|_| {
                    ExperimentError::InvalidConfig(format!("invalid r in params: {params}"))
                })?;
                r = Some(parsed);
            }
            "k" => {
                let parsed = value.parse::<usize>().map_err(|_| {
                    ExperimentError::InvalidConfig(format!("invalid k in params: {params}"))
                })?;
                k = Some(parsed);
            }
            "u" => {
                u = Some(value.to_string());
            }
            "v" => {
                v = Some(value.to_string());
            }
            _ => {}
        }
    }
    let r =
        r.ok_or_else(|| ExperimentError::InvalidConfig(format!("missing r in params: {params}")))?;
    let k =
        k.ok_or_else(|| ExperimentError::InvalidConfig(format!("missing k in params: {params}")))?;
    let u =
        u.ok_or_else(|| ExperimentError::InvalidConfig(format!("missing u in params: {params}")))?;
    let v =
        v.ok_or_else(|| ExperimentError::InvalidConfig(format!("missing v in params: {params}")))?;
    Ok((r, k, u, v))
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

fn build_matrix(
    u_order: &[String],
    v_order: &[String],
    values_map: &BTreeMap<PairKey, Vec<Coeff>>,
    loop_idx: usize,
) -> Vec<Vec<Coeff>> {
    let mut matrix = vec![vec![Coeff::zero(); v_order.len()]; u_order.len()];
    for (i, u) in u_order.iter().enumerate() {
        for (j, v) in v_order.iter().enumerate() {
            let key = PairKey {
                u: u.clone(),
                v: v.clone(),
            };
            if let Some(values) = values_map.get(&key) {
                if let Some(value) = values.get(loop_idx) {
                    matrix[i][j] = *value;
                }
            }
        }
    }
    matrix
}

fn rank_and_prime(
    matrix: &[Vec<Coeff>],
    primes: &[i64],
) -> Result<(usize, i64, Vec<Vec<i64>>), ExperimentError> {
    if matrix.is_empty() || matrix[0].is_empty() {
        let prime = *primes
            .first()
            .ok_or_else(|| ExperimentError::InvalidConfig("no primes supplied".to_string()))?;
        return Ok((0, prime, Vec::new()));
    }

    let mut best_rank = None;
    let mut best_prime = 0i64;
    let mut best_matrix = None;
    for &prime in primes {
        if let Some(mod_matrix) = matrix_to_mod(matrix, prime) {
            let rank = rank_mod_p_i64(&mod_matrix, prime);
            let replace = match best_rank {
                None => true,
                Some(current) => rank > current || (rank == current && prime < best_prime),
            };
            if replace {
                best_rank = Some(rank);
                best_prime = prime;
                best_matrix = Some(mod_matrix);
            }
        }
    }

    let Some(rank) = best_rank else {
        return Err(ExperimentError::InvalidConfig(
            "no usable primes for matrix mod-p rank".to_string(),
        ));
    };
    Ok((rank, best_prime, best_matrix.unwrap_or_default()))
}

fn matrix_to_mod(matrix: &[Vec<Coeff>], p: i64) -> Option<Vec<Vec<i64>>> {
    let mut out = Vec::with_capacity(matrix.len());
    for row in matrix {
        let mut row_out = Vec::with_capacity(row.len());
        for coeff in row {
            let denom = *coeff.denom();
            let denom_mod = mod_i64(denom, p);
            if denom_mod == 0 {
                return None;
            }
            let inv = mod_inv(denom_mod, p);
            let numer = mod_i64(*coeff.numer(), p);
            row_out.push(mod_i64(numer * inv, p));
        }
        out.push(row_out);
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

fn compute_dependencies_mod_p(
    matrix: &[Vec<i64>],
    row_ids: &[String],
    loop_index: usize,
    prime: i64,
) -> Result<Vec<HankelDependency>, ExperimentError> {
    if matrix.len() != row_ids.len() {
        return Err(ExperimentError::InvalidConfig(
            "row id count does not match matrix rows".to_string(),
        ));
    }
    if matrix.is_empty() {
        return Ok(Vec::new());
    }
    let ncols = matrix[0].len();
    for row in matrix {
        if row.len() != ncols {
            return Err(ExperimentError::InvalidConfig(
                "matrix row length mismatch".to_string(),
            ));
        }
    }

    let basis_indices = row_basis_indices_mod_p(matrix, prime);
    let mut basis_sorted = basis_indices.clone();
    basis_sorted.sort_unstable();
    let mut is_basis = vec![false; matrix.len()];
    for idx in &basis_sorted {
        if *idx < is_basis.len() {
            is_basis[*idx] = true;
        }
    }

    let mut basis_rows = Vec::with_capacity(basis_sorted.len());
    let mut basis_ids = Vec::with_capacity(basis_sorted.len());
    for idx in basis_sorted {
        basis_rows.push(matrix[idx].clone());
        basis_ids.push(row_ids[idx].clone());
    }

    let basis_t = transpose_mod_p(&basis_rows);
    let mut deps = Vec::new();
    for (row_idx, row) in matrix.iter().enumerate() {
        if is_basis[row_idx] {
            continue;
        }
        let terms = if basis_rows.is_empty() {
            if row.iter().all(|v| *v == 0) {
                Vec::new()
            } else {
                return Err(ExperimentError::InvalidConfig(format!(
                    "row {} is not in span of empty basis",
                    row_ids[row_idx]
                )));
            }
        } else {
            let solution = solve_linear_system_mod_p(&basis_t, row, prime).ok_or_else(|| {
                ExperimentError::InvalidConfig(format!(
                    "row {} is not in span of basis",
                    row_ids[row_idx]
                ))
            })?;
            let mut terms = Vec::new();
            for (coeff, id) in solution.iter().zip(basis_ids.iter()) {
                let value = mod_i64(*coeff, prime);
                if value == 0 {
                    continue;
                }
                terms.push((value, id.clone()));
            }
            terms
        };
        deps.push(HankelDependency {
            loop_index,
            prime,
            id: row_ids[row_idx].clone(),
            terms,
        });
    }
    Ok(deps)
}

fn row_basis_indices_mod_p(matrix: &[Vec<i64>], p: i64) -> Vec<usize> {
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
        let pivot_val = mod_i64(mat[row][col], p);
        let inv = mod_inv(pivot_val, p);
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
        row += 1;
        if row >= nrows {
            break;
        }
    }
    basis
}

fn solve_linear_system_mod_p(matrix: &[Vec<i64>], rhs: &[i64], p: i64) -> Option<Vec<i64>> {
    if matrix.is_empty() {
        return None;
    }
    let nrows = matrix.len();
    let ncols = matrix[0].len();
    if rhs.len() != nrows {
        return None;
    }

    let mut mat = vec![vec![0i64; ncols + 1]; nrows];
    for (r, row_vals) in matrix.iter().enumerate() {
        for (c, value) in row_vals.iter().enumerate() {
            mat[r][c] = mod_i64(*value, p);
        }
        mat[r][ncols] = mod_i64(rhs[r], p);
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
        let pivot_val = mod_i64(mat[row][col], p);
        let inv = mod_inv(pivot_val, p);
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
        if let Some(r) = pivot_row {
            solution[col] = mod_i64(mat[*r][ncols], p);
        }
    }
    Some(solution)
}

fn transpose_mod_p(matrix: &[Vec<i64>]) -> Vec<Vec<i64>> {
    if matrix.is_empty() {
        return Vec::new();
    }
    let nrows = matrix.len();
    let ncols = matrix[0].len();
    let mut out = vec![vec![0i64; nrows]; ncols];
    for (i, row) in matrix.iter().enumerate() {
        for (j, value) in row.iter().enumerate() {
            out[j][i] = *value;
        }
    }
    out
}

fn compare_dependency(a: &HankelDependency, b: &HankelDependency) -> std::cmp::Ordering {
    let loop_cmp = a.loop_index.cmp(&b.loop_index);
    if loop_cmp != std::cmp::Ordering::Equal {
        return loop_cmp;
    }
    let prime_cmp = a.prime.cmp(&b.prime);
    if prime_cmp != std::cmp::Ordering::Equal {
        return prime_cmp;
    }
    let id_cmp = a.id.cmp(&b.id);
    if id_cmp != std::cmp::Ordering::Equal {
        return id_cmp;
    }
    let terms_a = format_terms(&a.terms, a.prime);
    let terms_b = format_terms(&b.terms, b.prime);
    terms_a.cmp(&terms_b)
}

fn render_stats_csv(cfg: &EsymbHankelSubblockConfig, stats: &[HankelSubblockStats]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["loop", "r", "k", "nrows", "ncols", "rank_mod_p"]);
    for row in stats {
        writer.push_record([
            row.loop_index.to_string(),
            cfg.r.to_string(),
            cfg.k.to_string(),
            row.nrows.to_string(),
            row.ncols.to_string(),
            row.rank_mod_p.to_string(),
        ]);
    }
    writer.into_string()
}

fn render_deps_csv(
    cfg: &EsymbHankelSubblockConfig,
    deps: &[HankelDependency],
    label: &str,
) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record(["loop", "r", "k", "prime", label, "terms"]);
    for dep in deps {
        writer.push_record([
            dep.loop_index.to_string(),
            cfg.r.to_string(),
            cfg.k.to_string(),
            dep.prime.to_string(),
            dep.id.clone(),
            format_terms(&dep.terms, dep.prime),
        ]);
    }
    writer.into_string()
}

fn render_md(
    cfg: &EsymbHankelSubblockConfig,
    loops: &[usize],
    stats: &[HankelSubblockStats],
    row_deps: &[HankelDependency],
    col_deps: &[HankelDependency],
) -> String {
    let mut out = String::new();
    out.push_str("# esymb_hankel_subblock\n\n");
    out.push_str(&format!("loops = {}\n\n", format_usize_list(loops)));
    out.push_str(&format!("r = {}\n\n", cfg.r));
    out.push_str(&format!("k = {}\n\n", cfg.k));
    out.push_str(&format!("primes = {}\n\n", format_i64_list(&cfg.primes)));
    out.push_str(&format!("exact = {}\n\n", cfg.exact));

    out.push_str("## rank_summary\n\n");
    if stats.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        out.push_str("| loop | nrows | ncols | rank_mod_p |\n");
        out.push_str("| --- | --- | --- | --- |\n");
        let mut rows = stats.to_vec();
        rows.sort_by(|a, b| a.loop_index.cmp(&b.loop_index));
        for row in rows {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                row.loop_index, row.nrows, row.ncols, row.rank_mod_p
            ));
        }
        out.push('\n');
    }

    if cfg.exact {
        out.push_str("## sample_row_deps\n\n");
        render_sample_deps(&mut out, row_deps);
        out.push_str("\n## sample_col_deps\n\n");
        render_sample_deps(&mut out, col_deps);
        out.push_str("\n## state_merge_note\n\n");
        out.push_str("Dependencies are computed over a finite field using the smallest prime that attains the max mod-p rank per loop. Rows or columns that are identical or proportional mod p suggest candidate state merges for prefix/suffix blocks.\n");
    }
    out
}

fn render_sample_deps(out: &mut String, deps: &[HankelDependency]) {
    if deps.is_empty() {
        out.push_str("_none_\n");
        return;
    }
    let mut rows = deps.to_vec();
    rows.sort_by(compare_dep_for_sample);
    let limit = 20usize.min(rows.len());
    for dep in rows.into_iter().take(limit) {
        let terms = format_terms(&dep.terms, dep.prime);
        out.push_str(&format!(
            "- L{} (p={}) {} = {}\n",
            dep.loop_index, dep.prime, dep.id, terms
        ));
    }
}

fn compare_dep_for_sample(a: &HankelDependency, b: &HankelDependency) -> std::cmp::Ordering {
    let support_cmp = a.terms.len().cmp(&b.terms.len());
    if support_cmp != std::cmp::Ordering::Equal {
        return support_cmp;
    }
    compare_dependency(a, b)
}

fn format_terms(terms: &[(i64, String)], prime: i64) -> String {
    if terms.is_empty() {
        return "0".to_string();
    }
    terms
        .iter()
        .map(|(coeff, id)| format!("{}*{}", format_mod_coeff(*coeff, prime), id))
        .collect::<Vec<_>>()
        .join(" + ")
}

fn format_mod_coeff(value: i64, prime: i64) -> String {
    if prime == 0 {
        return value.to_string();
    }
    let mut v = mod_i64(value, prime);
    let half = prime / 2;
    if v > half {
        v -= prime;
    }
    v.to_string()
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

fn format_i64_list(values: &[i64]) -> String {
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
