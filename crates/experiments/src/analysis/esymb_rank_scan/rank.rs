use mpl_symbol::Coeff;

use crate::ExperimentError;

pub fn compute_nmax(len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (len - 1) / 2
    }
}

pub fn rank_curve_mod_p(
    values: &[Coeff],
    primes: &[i64],
    nmax: usize,
) -> Result<Vec<usize>, ExperimentError> {
    let mut out = Vec::with_capacity(nmax + 1);
    let mod_values = primes
        .iter()
        .filter_map(|&p| values_to_mod(values, p).map(|vals| (p, vals)))
        .collect::<Vec<_>>();
    if mod_values.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "no usable primes for mod-p rank".to_string(),
        ));
    }
    for n in 0..=nmax {
        let m = n + 1;
        let mut ranks = Vec::new();
        for (p, vals) in &mod_values {
            if 2 * n >= vals.len() {
                continue;
            }
            let matrix = hankel_mod(vals, m);
            let rank = rank_mod_p(&matrix, *p);
            ranks.push(rank);
        }
        let estimate = ranks.into_iter().max().unwrap_or(0);
        out.push(estimate);
    }
    Ok(out)
}

pub fn rank_curve_subsample(
    values: &[Coeff],
    primes: &[i64],
    nmax: usize,
    sample_size: usize,
    seed: u64,
) -> Result<Vec<usize>, ExperimentError> {
    let mut out = Vec::with_capacity(nmax + 1);
    let mod_values = primes
        .iter()
        .filter_map(|&p| values_to_mod(values, p).map(|vals| (p, vals)))
        .collect::<Vec<_>>();
    if mod_values.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "no usable primes for mod-p rank".to_string(),
        ));
    }
    for n in 0..=nmax {
        let m = n + 1;
        let k = sample_size.min(m);
        let rows = sample_indices(m, k, seed);
        let cols = sample_indices(m, k, seed.wrapping_add(1));
        let mut ranks = Vec::new();
        for (p, vals) in &mod_values {
            if 2 * n >= vals.len() {
                continue;
            }
            let matrix = hankel_mod(vals, m);
            let sub = submatrix(&matrix, &rows, &cols);
            ranks.push(rank_mod_p(&sub, *p));
        }
        let estimate = ranks.into_iter().max().unwrap_or(0);
        out.push(estimate);
    }
    Ok(out)
}

pub fn rank_curve_float(values: &[Coeff], nmax: usize, tau: f64) -> Vec<usize> {
    let floats = values.iter().map(coeff_to_f64).collect::<Vec<_>>();
    let mut out = Vec::with_capacity(nmax + 1);
    for n in 0..=nmax {
        let m = n + 1;
        if 2 * n >= floats.len() {
            out.push(0);
            continue;
        }
        let matrix = hankel_f64(&floats, m);
        out.push(rank_float(&matrix, tau));
    }
    out
}

pub fn rank_matrix_mod_p(matrix: &[Vec<Coeff>], primes: &[i64]) -> Result<usize, ExperimentError> {
    if matrix.is_empty() || matrix[0].is_empty() {
        return Ok(0);
    }
    let mod_matrices = primes
        .iter()
        .filter_map(|&p| matrix_to_mod(matrix, p).map(|vals| (p, vals)))
        .collect::<Vec<_>>();
    if mod_matrices.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "no usable primes for matrix mod-p rank".to_string(),
        ));
    }
    let mut ranks = Vec::new();
    for (p, mat) in &mod_matrices {
        ranks.push(rank_mod_p(mat, *p));
    }
    Ok(ranks.into_iter().max().unwrap_or(0))
}

pub fn detect_plateau(curve: &[usize], len: usize) -> Option<usize> {
    if curve.len() < len || len == 0 {
        return None;
    }
    let tail = &curve[curve.len() - len..];
    if tail.windows(2).all(|w| w[0] == w[1]) {
        return tail.last().copied();
    }
    None
}

fn values_to_mod(values: &[Coeff], p: i64) -> Option<Vec<i64>> {
    let mut out = Vec::with_capacity(values.len());
    for coeff in values {
        let denom = *coeff.denom();
        let denom_mod = mod_i64(denom, p);
        if denom_mod == 0 {
            return None;
        }
        let inv = mod_inv(denom_mod, p);
        let numer = mod_i64(*coeff.numer(), p);
        out.push(mod_i64(numer * inv, p));
    }
    Some(out)
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

fn hankel_mod(values: &[i64], size: usize) -> Vec<Vec<i64>> {
    let mut matrix = vec![vec![0; size]; size];
    for (i, row) in matrix.iter_mut().enumerate() {
        row.copy_from_slice(&values[i..i + size]);
    }
    matrix
}

fn hankel_f64(values: &[f64], size: usize) -> Vec<Vec<f64>> {
    let mut matrix = vec![vec![0.0; size]; size];
    for (i, row) in matrix.iter_mut().enumerate() {
        row.copy_from_slice(&values[i..i + size]);
    }
    matrix
}

fn rank_mod_p(matrix: &[Vec<i64>], p: i64) -> usize {
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
            if row_vals[col] % p != 0 {
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

fn rank_float(matrix: &[Vec<f64>], tau: f64) -> usize {
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
        let mut best = 0.0;
        for (r, row_vals) in mat.iter().enumerate().skip(row) {
            let value = row_vals[col].abs();
            if value > best {
                best = value;
                pivot = Some(r);
            }
        }
        let Some(pivot_row) = pivot else {
            continue;
        };
        if best <= tau {
            continue;
        }
        mat.swap(row, pivot_row);
        let pivot_val = mat[row][col];
        for value in mat[row].iter_mut().skip(col) {
            *value /= pivot_val;
        }
        let pivot_row_vals = mat[row].clone();
        for (r, row_vals) in mat.iter_mut().enumerate() {
            if r == row {
                continue;
            }
            let factor = row_vals[col];
            if factor.abs() <= tau {
                continue;
            }
            for (c, value) in row_vals.iter_mut().enumerate().skip(col) {
                *value -= factor * pivot_row_vals[c];
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

fn coeff_to_f64(coeff: &Coeff) -> f64 {
    let numer = *coeff.numer() as f64;
    let denom = *coeff.denom() as f64;
    if denom == 0.0 {
        0.0
    } else {
        numer / denom
    }
}

fn mod_i64(value: i64, p: i64) -> i64 {
    let mut v = value % p;
    if v < 0 {
        v += p;
    }
    v
}

fn mod_inv(value: i64, p: i64) -> i64 {
    mod_pow(value, p - 2, p)
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

fn sample_indices(n: usize, k: usize, seed: u64) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..n).collect();
    let mut state = seed;
    for i in (1..n).rev() {
        state = lcg_next(state);
        let j = (state % (i as u64 + 1)) as usize;
        indices.swap(i, j);
    }
    indices.truncate(k);
    indices.sort_unstable();
    indices
}

fn lcg_next(state: u64) -> u64 {
    state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

fn submatrix(matrix: &[Vec<i64>], rows: &[usize], cols: &[usize]) -> Vec<Vec<i64>> {
    let mut out = Vec::with_capacity(rows.len());
    for &r in rows {
        let mut row = Vec::with_capacity(cols.len());
        for &c in cols {
            row.push(matrix[r][c]);
        }
        out.push(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_curve_constant_sequence() {
        let values = vec![
            Coeff::from_integer(1),
            Coeff::from_integer(1),
            Coeff::from_integer(1),
            Coeff::from_integer(1),
        ];
        let nmax = compute_nmax(values.len());
        let curve = rank_curve_mod_p(&values, &[101], nmax).expect("rank curve");
        assert_eq!(curve, vec![1, 1]);
    }
}
