use mpl_symbol::Coeff;

use crate::ExperimentError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizeMode {
    None,
    OddDoubleFactorial,
    EvenDoubleFactorial,
    FactorialLm1,
    CentralBinomialLm1,
}

impl NormalizeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OddDoubleFactorial => "odd-double-factorial",
            Self::EvenDoubleFactorial => "even-double-factorial",
            Self::FactorialLm1 => "factorial",
            Self::CentralBinomialLm1 => "central-binomial",
        }
    }

    pub fn order_key(self) -> usize {
        match self {
            Self::None => 0,
            Self::OddDoubleFactorial => 1,
            Self::EvenDoubleFactorial => 2,
            Self::FactorialLm1 => 3,
            Self::CentralBinomialLm1 => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizeChoice {
    None,
    OddDoubleFactorial,
    EvenDoubleFactorial,
    FactorialLm1,
    CentralBinomialLm1,
    Auto,
}

impl NormalizeChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OddDoubleFactorial => "odd-double-factorial",
            Self::EvenDoubleFactorial => "even-double-factorial",
            Self::FactorialLm1 => "factorial",
            Self::CentralBinomialLm1 => "central-binomial",
            Self::Auto => "auto",
        }
    }
}

pub fn normalize_values(
    values: &[Coeff],
    loops: &[usize],
    mode: NormalizeMode,
) -> Result<Vec<Coeff>, ExperimentError> {
    if values.len() != loops.len() {
        return Err(ExperimentError::InvalidConfig(
            "normalize values: length mismatch".to_string(),
        ));
    }
    match mode {
        NormalizeMode::None => Ok(values.to_vec()),
        NormalizeMode::OddDoubleFactorial => {
            let mut out = Vec::with_capacity(values.len());
            for (&loop_index, value) in loops.iter().zip(values.iter()) {
                let factor = odd_double_factorial(loop_index)?;
                out.push(*value / factor);
            }
            Ok(out)
        }
        NormalizeMode::EvenDoubleFactorial => {
            let mut out = Vec::with_capacity(values.len());
            for (&loop_index, value) in loops.iter().zip(values.iter()) {
                let factor = even_double_factorial(loop_index)?;
                out.push(*value / factor);
            }
            Ok(out)
        }
        NormalizeMode::FactorialLm1 => {
            let mut out = Vec::with_capacity(values.len());
            for (&loop_index, value) in loops.iter().zip(values.iter()) {
                let factor = factorial_lm1(loop_index)?;
                out.push(*value / factor);
            }
            Ok(out)
        }
        NormalizeMode::CentralBinomialLm1 => {
            let mut out = Vec::with_capacity(values.len());
            for (&loop_index, value) in loops.iter().zip(values.iter()) {
                let factor = central_binomial_lm1(loop_index)?;
                out.push(*value / factor);
            }
            Ok(out)
        }
    }
}

pub fn odd_double_factorial(loop_index: usize) -> Result<Coeff, ExperimentError> {
    if loop_index <= 1 {
        return Ok(Coeff::from_integer(1));
    }
    let mut acc: i64 = 1;
    for k in 1..loop_index {
        let term = (2 * k - 1) as i64;
        acc = acc.checked_mul(term).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!(
                "odd double factorial overflow for L={loop_index}"
            ))
        })?;
    }
    Ok(Coeff::from_integer(acc))
}

pub fn even_double_factorial(loop_index: usize) -> Result<Coeff, ExperimentError> {
    if loop_index <= 1 {
        return Ok(Coeff::from_integer(1));
    }
    let mut acc: i64 = 1;
    for k in 1..loop_index {
        let term = (2 * k) as i64;
        acc = acc.checked_mul(term).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!(
                "even double factorial overflow for L={loop_index}"
            ))
        })?;
    }
    Ok(Coeff::from_integer(acc))
}

pub fn factorial_lm1(loop_index: usize) -> Result<Coeff, ExperimentError> {
    if loop_index <= 1 {
        return Ok(Coeff::from_integer(1));
    }
    let mut acc: i64 = 1;
    for k in 1..loop_index {
        acc = acc.checked_mul(k as i64).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!("factorial overflow for L={loop_index}"))
        })?;
    }
    Ok(Coeff::from_integer(acc))
}

pub fn central_binomial_lm1(loop_index: usize) -> Result<Coeff, ExperimentError> {
    if loop_index <= 1 {
        return Ok(Coeff::from_integer(1));
    }
    let n = loop_index - 1;
    let mut result: i128 = 1;
    for k in 1..=n {
        let mut numer = (n + k) as i128;
        let mut denom = k as i128;
        let g1 = gcd_i128(numer, denom);
        numer /= g1;
        denom /= g1;
        let g2 = gcd_i128(result, denom);
        result /= g2;
        denom /= g2;
        if denom != 1 {
            return Err(ExperimentError::InvalidConfig(format!(
                "central binomial reduction failed for L={loop_index}"
            )));
        }
        result = result.checked_mul(numer).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!("central binomial overflow for L={loop_index}"))
        })?;
        if result > i64::MAX as i128 {
            return Err(ExperimentError::InvalidConfig(format!(
                "central binomial overflow for L={loop_index}"
            )));
        }
    }
    Ok(Coeff::from_integer(result as i64))
}

fn gcd_i128(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::esymb_rank_scan::rank;
    use crate::analysis::esymb_rank_scan::solve;

    #[test]
    fn odd_double_factorial_values() {
        let expected = [1, 1, 3, 15, 105, 945];
        for (idx, &value) in expected.iter().enumerate() {
            let loop_index = idx + 1;
            let coeff = odd_double_factorial(loop_index).expect("odd double factorial");
            assert_eq!(coeff, Coeff::from_integer(value));
        }
    }

    #[test]
    fn even_double_factorial_values() {
        let expected = [1, 2, 8, 48, 384, 3840];
        for (idx, &value) in expected.iter().enumerate() {
            let loop_index = idx + 1;
            let coeff = even_double_factorial(loop_index).expect("even double factorial");
            assert_eq!(coeff, Coeff::from_integer(value));
        }
    }

    #[test]
    fn central_binomial_values() {
        let expected = [1, 2, 6, 20, 70, 252];
        for (idx, &value) in expected.iter().enumerate() {
            let loop_index = idx + 1;
            let coeff = central_binomial_lm1(loop_index).expect("central binomial");
            assert_eq!(coeff, Coeff::from_integer(value));
        }
    }

    #[test]
    fn normalize_and_rank_plateau() {
        let loops = vec![1, 2, 3, 4, 5, 6];
        let values = vec![
            Coeff::from_integer(-2),
            Coeff::from_integer(16),
            Coeff::from_integer(-384),
            Coeff::from_integer(15360),
            Coeff::from_integer(-860160),
            Coeff::from_integer(61931520),
        ];
        let normalized = normalize_values(&values, &loops, NormalizeMode::OddDoubleFactorial)
            .expect("normalize");
        let expected = vec![
            Coeff::from_integer(-2),
            Coeff::from_integer(16),
            Coeff::from_integer(-128),
            Coeff::from_integer(1024),
            Coeff::from_integer(-8192),
            Coeff::from_integer(65536),
        ];
        assert_eq!(normalized, expected);
        let nmax = rank::compute_nmax(normalized.len());
        let curve = rank::rank_curve_mod_p(&normalized, &[101], nmax).expect("rank");
        assert_eq!(curve, vec![1, 1, 1]);
    }

    #[test]
    fn solve_normalized_recurrence() {
        let normalized = vec![
            Coeff::from_integer(-2),
            Coeff::from_integer(16),
            Coeff::from_integer(-128),
            Coeff::from_integer(1024),
            Coeff::from_integer(-8192),
            Coeff::from_integer(65536),
        ];
        let rec = solve::solve_recurrence(&normalized, 1, 0).expect("recurrence");
        let rho = -rec.coeffs[0];
        assert_eq!(rho, Coeff::from_integer(-8));
    }
}
