use mpl_symbol::Coeff;
use num_traits::{One, Zero};

#[derive(Clone, Debug)]
pub struct RecurrenceCandidate {
    pub order: usize,
    pub coeffs: Vec<Coeff>,
}

pub fn solve_recurrence(
    values: &[Coeff],
    order: usize,
    offset: usize,
) -> Option<RecurrenceCandidate> {
    if order == 0 {
        return None;
    }
    let needed = offset + order.saturating_mul(2);
    if values.len() < needed {
        return None;
    }
    let rows = order;
    let cols = order + 1;
    let mut matrix = vec![vec![Coeff::zero(); cols]; rows];
    for (r, row_vals) in matrix.iter_mut().enumerate() {
        for (c, value) in row_vals.iter_mut().enumerate() {
            *value = values[offset + r + c];
        }
    }
    let mut coeffs = kernel_vector(matrix)?;
    let last_idx = last_nonzero_index(&coeffs)?;
    if last_idx == 0 {
        return None;
    }
    let scale = coeffs[last_idx];
    if scale.is_zero() {
        return None;
    }
    let inv = Coeff::one() / scale;
    for value in &mut coeffs {
        *value *= inv;
    }
    coeffs.truncate(last_idx + 1);
    Some(RecurrenceCandidate {
        order: last_idx,
        coeffs,
    })
}

pub fn equivalent_recurrence(a: &RecurrenceCandidate, b: &RecurrenceCandidate) -> bool {
    if a.order != b.order {
        return false;
    }
    let Some(norm_a) = normalize_recurrence(&a.coeffs) else {
        return false;
    };
    let Some(norm_b) = normalize_recurrence(&b.coeffs) else {
        return false;
    };
    norm_a == norm_b
}

pub fn verify_recurrence(values: &[Coeff], recurrence: &RecurrenceCandidate) -> bool {
    let order = recurrence.order;
    if order == 0 || values.len() <= order {
        return false;
    }
    let coeffs = &recurrence.coeffs;
    let limit = values.len() - order;
    for n in 0..limit {
        let mut sum = Coeff::zero();
        for k in 0..=order {
            sum += coeffs[k] * values[n + k];
        }
        if !sum.is_zero() {
            return false;
        }
    }
    true
}

pub fn predict_next_value(values: &[Coeff], recurrence: &RecurrenceCandidate) -> Option<Coeff> {
    let order = recurrence.order;
    if order == 0 || values.len() < order {
        return None;
    }
    let coeffs = &recurrence.coeffs;
    let start = values.len().saturating_sub(order);
    let mut sum = Coeff::zero();
    for k in 0..order {
        sum += coeffs[k] * values[start + k];
    }
    Some(-sum)
}

fn kernel_vector(mut matrix: Vec<Vec<Coeff>>) -> Option<Vec<Coeff>> {
    let rows = matrix.len();
    if rows == 0 {
        return None;
    }
    let cols = matrix[0].len();
    let mut pivot_cols = Vec::new();
    let mut row = 0usize;
    for col in 0..cols {
        if row >= rows {
            break;
        }
        let mut pivot = None;
        for (r, row_vals) in matrix.iter().enumerate().skip(row) {
            if !row_vals[col].is_zero() {
                pivot = Some(r);
                break;
            }
        }
        let Some(pivot_row) = pivot else {
            continue;
        };
        matrix.swap(row, pivot_row);
        let pivot_val = matrix[row][col];
        for value in matrix[row].iter_mut().skip(col) {
            *value /= pivot_val;
        }
        let pivot_snapshot = matrix[row].clone();
        for (r, row_vals) in matrix.iter_mut().enumerate() {
            if r == row {
                continue;
            }
            let factor = row_vals[col];
            if factor.is_zero() {
                continue;
            }
            for (c, value) in row_vals.iter_mut().enumerate().skip(col) {
                *value -= factor * pivot_snapshot[c];
            }
        }
        pivot_cols.push(col);
        row += 1;
    }

    let mut free_cols = Vec::new();
    for col in 0..cols {
        if !pivot_cols.contains(&col) {
            free_cols.push(col);
        }
    }
    let free_col = *free_cols.last()?;

    let mut vec = vec![Coeff::zero(); cols];
    vec[free_col] = Coeff::one();
    for (pivot_row, &pivot_col) in pivot_cols.iter().enumerate() {
        let coeff = matrix[pivot_row][free_col];
        if !coeff.is_zero() {
            vec[pivot_col] = -coeff;
        }
    }
    Some(vec)
}

fn last_nonzero_index(values: &[Coeff]) -> Option<usize> {
    for (idx, value) in values.iter().enumerate().rev() {
        if !value.is_zero() {
            return Some(idx);
        }
    }
    None
}

fn normalize_recurrence(values: &[Coeff]) -> Option<Vec<Coeff>> {
    let last_idx = last_nonzero_index(values)?;
    let scale = values[last_idx];
    if scale.is_zero() {
        return None;
    }
    let inv = Coeff::one() / scale;
    let mut out = Vec::with_capacity(last_idx + 1);
    for value in values.iter().take(last_idx + 1) {
        out.push(*value * inv);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_fibonacci_recurrence() {
        let values = vec![
            Coeff::from_integer(1),
            Coeff::from_integer(1),
            Coeff::from_integer(2),
            Coeff::from_integer(3),
            Coeff::from_integer(5),
            Coeff::from_integer(8),
        ];
        let rec = solve_recurrence(&values, 2, 0).expect("recurrence");
        assert_eq!(rec.order, 2);
        assert!(verify_recurrence(&values, &rec));
        let next = predict_next_value(&values, &rec).expect("next");
        assert_eq!(next, Coeff::from_integer(13));
    }
}
