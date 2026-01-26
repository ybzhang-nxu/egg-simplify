use crate::ExperimentError;

use super::field::{Field, Fp};

pub type Matrix = Vec<Vec<Fp>>;
pub type Vector = Vec<Fp>;

pub fn zeros(rows: usize, cols: usize, field: &Field) -> Matrix {
    vec![vec![field.zero(); cols]; rows]
}

pub fn identity(size: usize, field: &Field) -> Matrix {
    let mut out = zeros(size, size, field);
    for (idx, row) in out.iter_mut().enumerate() {
        row[idx] = field.one();
    }
    out
}

pub fn dot(left: &[Fp], right: &[Fp], field: &Field) -> Fp {
    let mut acc = field.zero();
    for idx in 0..left.len() {
        acc = field.add(acc, field.mul(left[idx], right[idx]));
    }
    acc
}

pub fn vec_mat_mul(vec: &[Fp], mat: &Matrix, field: &Field) -> Vector {
    debug_assert!(mat.len() == vec.len());
    if mat.is_empty() {
        return Vec::new();
    }
    let cols = mat[0].len();
    let mut out = vec![field.zero(); cols];
    for (row_idx, &value) in vec.iter().enumerate() {
        if value.is_zero() {
            continue;
        }
        for col in 0..cols {
            let prod = field.mul(value, mat[row_idx][col]);
            out[col] = field.add(out[col], prod);
        }
    }
    out
}

pub fn mat_vec_mul(mat: &Matrix, vec: &[Fp], field: &Field) -> Vector {
    debug_assert!(mat.is_empty() || mat[0].len() == vec.len());
    let mut out = vec![field.zero(); mat.len()];
    for (row_idx, row) in mat.iter().enumerate() {
        out[row_idx] = dot(row, vec, field);
    }
    out
}

pub fn mat_mul(left: &Matrix, right: &Matrix, field: &Field) -> Matrix {
    if left.is_empty() {
        return Vec::new();
    }
    debug_assert!(left[0].len() == right.len());
    let rows = left.len();
    let cols = right[0].len();
    let mut out = zeros(rows, cols, field);
    for row in 0..rows {
        for k in 0..right.len() {
            let value = left[row][k];
            if value.is_zero() {
                continue;
            }
            for col in 0..cols {
                let prod = field.mul(value, right[k][col]);
                out[row][col] = field.add(out[row][col], prod);
            }
        }
    }
    out
}

pub fn select_rows(mat: &Matrix, rows: &[usize]) -> Matrix {
    let mut out = Vec::with_capacity(rows.len());
    for &idx in rows {
        out.push(mat[idx].clone());
    }
    out
}

pub fn select_cols(mat: &Matrix, cols: &[usize]) -> Matrix {
    let mut out = Vec::with_capacity(mat.len());
    for row in mat {
        let mut next = Vec::with_capacity(cols.len());
        for &col in cols {
            next.push(row[col]);
        }
        out.push(next);
    }
    out
}

pub fn select_col(mat: &Matrix, col: usize) -> Vector {
    mat.iter().map(|row| row[col]).collect()
}

pub fn selection_rows(indices: &[usize], total_rows: usize, field: &Field) -> Matrix {
    let mut out = Vec::with_capacity(indices.len());
    for &idx in indices {
        let mut row = vec![field.zero(); total_rows];
        row[idx] = field.one();
        out.push(row);
    }
    out
}

pub fn selection_cols(indices: &[usize], total_cols: usize, field: &Field) -> Matrix {
    let mut out = vec![vec![field.zero(); indices.len()]; total_cols];
    for (col_idx, &row_idx) in indices.iter().enumerate() {
        out[row_idx][col_idx] = field.one();
    }
    out
}

pub fn pivot_columns(mat: &Matrix, field: &Field) -> Result<Vec<usize>, ExperimentError> {
    if mat.is_empty() {
        return Ok(Vec::new());
    }
    let rows = mat.len();
    let cols = mat[0].len();
    let mut work = mat.clone();
    let mut pivot_row = 0usize;
    let mut pivots = Vec::new();

    for col in 0..cols {
        let mut row = pivot_row;
        while row < rows && work[row][col].is_zero() {
            row += 1;
        }
        if row == rows {
            continue;
        }
        work.swap(pivot_row, row);
        let pivot = work[pivot_row][col];
        let inv = field.inv(pivot).ok_or_else(|| {
            ExperimentError::InvalidConfig("non-invertible pivot; check prime".to_string())
        })?;
        for value in work[pivot_row].iter_mut().skip(col) {
            *value = field.mul(*value, inv);
        }
        for r in (pivot_row + 1)..rows {
            let factor = work[r][col];
            if factor.is_zero() {
                continue;
            }
            let (upper, lower) = work.split_at_mut(r);
            let pivot_slice = &upper[pivot_row];
            let row_slice = &mut lower[0];
            for (cell, pivot_val) in row_slice
                .iter_mut()
                .skip(col)
                .zip(pivot_slice.iter().skip(col))
            {
                let sub = field.mul(factor, *pivot_val);
                *cell = field.sub(*cell, sub);
            }
        }
        pivots.push(col);
        pivot_row += 1;
        if pivot_row == rows {
            break;
        }
    }

    Ok(pivots)
}

pub fn independent_row_indices(mat: &Matrix, field: &Field) -> Result<Vec<usize>, ExperimentError> {
    if mat.is_empty() {
        return Ok(Vec::new());
    }
    let rows = mat.len();
    let cols = mat[0].len();
    let mut work = mat.clone();
    let mut row_ids: Vec<usize> = (0..rows).collect();
    let mut pivot_row = 0usize;
    let mut pivots = Vec::new();

    for col in 0..cols {
        let mut row = pivot_row;
        while row < rows && work[row][col].is_zero() {
            row += 1;
        }
        if row == rows {
            continue;
        }
        work.swap(pivot_row, row);
        row_ids.swap(pivot_row, row);
        let pivot = work[pivot_row][col];
        let inv = field.inv(pivot).ok_or_else(|| {
            ExperimentError::InvalidConfig("non-invertible pivot; check prime".to_string())
        })?;
        for value in work[pivot_row].iter_mut().skip(col) {
            *value = field.mul(*value, inv);
        }
        for r in (pivot_row + 1)..rows {
            let factor = work[r][col];
            if factor.is_zero() {
                continue;
            }
            let (upper, lower) = work.split_at_mut(r);
            let pivot_slice = &upper[pivot_row];
            let row_slice = &mut lower[0];
            for (cell, pivot_val) in row_slice
                .iter_mut()
                .skip(col)
                .zip(pivot_slice.iter().skip(col))
            {
                let sub = field.mul(factor, *pivot_val);
                *cell = field.sub(*cell, sub);
            }
        }
        pivots.push(row_ids[pivot_row]);
        pivot_row += 1;
        if pivot_row == rows {
            break;
        }
    }

    Ok(pivots)
}

pub fn invert(mat: &Matrix, field: &Field) -> Result<Matrix, ExperimentError> {
    let n = mat.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    if mat[0].len() != n {
        return Err(ExperimentError::InvalidConfig(
            "matrix not square".to_string(),
        ));
    }

    let mut work = mat.clone();
    let mut inv = identity(n, field);

    for col in 0..n {
        let mut pivot = col;
        while pivot < n && work[pivot][col].is_zero() {
            pivot += 1;
        }
        if pivot == n {
            return Err(ExperimentError::InvalidConfig(
                "matrix not invertible".to_string(),
            ));
        }
        work.swap(col, pivot);
        inv.swap(col, pivot);

        let pivot_val = work[col][col];
        let inv_pivot = field.inv(pivot_val).ok_or_else(|| {
            ExperimentError::InvalidConfig("non-invertible pivot; check prime".to_string())
        })?;
        for j in 0..n {
            work[col][j] = field.mul(work[col][j], inv_pivot);
            inv[col][j] = field.mul(inv[col][j], inv_pivot);
        }
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = work[row][col];
            if factor.is_zero() {
                continue;
            }
            for j in 0..n {
                let sub = field.mul(factor, work[col][j]);
                work[row][j] = field.sub(work[row][j], sub);
                let sub_inv = field.mul(factor, inv[col][j]);
                inv[row][j] = field.sub(inv[row][j], sub_inv);
            }
        }
    }

    Ok(inv)
}
