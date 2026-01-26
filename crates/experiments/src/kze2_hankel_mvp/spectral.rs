use crate::ExperimentError;

use super::field::Field;
use super::linalg_modp::{
    independent_row_indices, invert, mat_mul, mat_vec_mul, pivot_columns, select_col, select_cols,
    select_rows, selection_cols, selection_rows, vec_mat_mul, Matrix, Vector,
};

#[derive(Clone, Debug)]
pub struct SpectralModel {
    pub rank: usize,
    pub alpha: Vector,
    pub beta: Vector,
    pub transitions: Vec<Matrix>,
}

pub fn spectral_learn(
    hankel: &Matrix,
    hankel_shifted: &[Matrix],
    field: &Field,
) -> Result<SpectralModel, ExperimentError> {
    if hankel.is_empty() || hankel[0].is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "empty hankel matrix".to_string(),
        ));
    }

    let pivot_cols = pivot_columns(hankel, field)?;
    let rank = pivot_cols.len();
    if rank == 0 {
        return Err(ExperimentError::InvalidConfig(
            "hankel rank is zero".to_string(),
        ));
    }

    let pm = select_cols(hankel, &pivot_cols);
    let row_indices = independent_row_indices(&pm, field)?;
    if row_indices.len() != rank {
        return Err(ExperimentError::InvalidConfig(
            "insufficient independent rows in Pm".to_string(),
        ));
    }
    let r_mat = select_rows(&pm, &row_indices);
    let inv_r = invert(&r_mat, field)?;
    let sel_rows = selection_rows(&row_indices, hankel.len(), field);
    let l_left = mat_mul(&inv_r, &sel_rows, field);

    let sm = mat_mul(&l_left, hankel, field);
    let pivot_cols_sm = pivot_columns(&sm, field)?;
    if pivot_cols_sm.len() != rank {
        return Err(ExperimentError::InvalidConfig(
            "insufficient independent cols in Sm".to_string(),
        ));
    }
    let c_mat = select_cols(&sm, &pivot_cols_sm);
    let inv_c = invert(&c_mat, field)?;
    let sel_cols = selection_cols(&pivot_cols_sm, hankel[0].len(), field);
    let r_right = mat_mul(&sel_cols, &inv_c, field);

    let mut transitions = Vec::with_capacity(hankel_shifted.len());
    for ha in hankel_shifted {
        let left = mat_mul(&l_left, ha, field);
        let mat = mat_mul(&left, &r_right, field);
        transitions.push(mat);
    }

    let alpha = vec_mat_mul(&hankel[0], &r_right, field);
    let h_col0 = select_col(hankel, 0);
    let beta = mat_vec_mul(&l_left, &h_col0, field);

    Ok(SpectralModel {
        rank,
        alpha,
        beta,
        transitions,
    })
}
