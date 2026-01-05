use std::collections::HashMap;

use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::{One, Zero};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("unknown variable '{0}'")]
    UnknownVariable(String),
    #[error("negative exponent on zero")]
    NegativePowerOfZero,
    #[error("overflow while computing power")]
    PowerOverflow,
    #[error("unsupported function '{0}'")]
    UnsupportedFunction(String),
}

pub fn eval_rational(
    expr: &Expr,
    env: &HashMap<String, Rational64>,
) -> Result<Rational64, EvalError> {
    match expr {
        Expr::Rational(value) => Ok(*value),
        Expr::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UnknownVariable(name.clone())),
        Expr::Add(children) => {
            let mut sum = Rational64::zero();
            for child in children {
                sum += eval_rational(child, env)?;
            }
            Ok(sum)
        }
        Expr::Mul(children) => {
            let mut product = Rational64::one();
            for child in children {
                product *= eval_rational(child, env)?;
            }
            Ok(product)
        }
        Expr::Neg(inner) => Ok(-eval_rational(inner, env)?),
        Expr::Pow(base, exp) => {
            let value = eval_rational(base, env)?;
            eval_pow(value, *exp)
        }
        Expr::Log(_) => Err(EvalError::UnsupportedFunction("log".to_string())),
        Expr::Li2(_) => Err(EvalError::UnsupportedFunction("li2".to_string())),
    }
}

fn eval_pow(value: Rational64, exp: i32) -> Result<Rational64, EvalError> {
    if exp == 0 {
        return Ok(Rational64::one());
    }
    if exp == i32::MIN {
        return Err(EvalError::PowerOverflow);
    }
    let numer = *value.numer();
    let denom = *value.denom();
    if exp < 0 && numer == 0 {
        return Err(EvalError::NegativePowerOfZero);
    }
    let exp_abs = exp.unsigned_abs();
    let (base_numer, base_denom) = if exp >= 0 {
        (numer, denom)
    } else {
        (denom, numer)
    };
    let numer_pow = base_numer
        .checked_pow(exp_abs)
        .ok_or(EvalError::PowerOverflow)?;
    let denom_pow = base_denom
        .checked_pow(exp_abs)
        .ok_or(EvalError::PowerOverflow)?;
    Ok(Rational64::new(numer_pow, denom_pow))
}

pub fn equiv_on_samples(a: &Expr, b: &Expr, samples: Vec<HashMap<String, Rational64>>) -> bool {
    for sample in samples {
        match (eval_rational(a, &sample), eval_rational(b, &sample)) {
            (Ok(left), Ok(right)) if left == right => {}
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpl_ir::parse_sexpr;
    use num_rational::Rational64;

    fn r(numer: i64, denom: i64) -> Rational64 {
        Rational64::new(numer, denom)
    }

    #[test]
    fn eval_with_env() {
        let expr = parse_sexpr("(+ (* 2 x) 1/2)").expect("parse");
        let mut env = HashMap::new();
        env.insert("x".to_string(), r(1, 4));
        let value = eval_rational(&expr, &env).expect("eval");
        assert_eq!(value, r(1, 1));
    }

    #[test]
    fn equiv_samples_match() {
        let left = parse_sexpr("(+ x y)").expect("parse");
        let right = parse_sexpr("(+ y x)").expect("parse");

        let mut sample1 = HashMap::new();
        sample1.insert("x".to_string(), r(1, 1));
        sample1.insert("y".to_string(), r(2, 1));

        let mut sample2 = HashMap::new();
        sample2.insert("x".to_string(), r(1, 2));
        sample2.insert("y".to_string(), r(4, 1));

        assert!(equiv_on_samples(&left, &right, vec![sample1, sample2]));
    }

    #[test]
    fn equiv_samples_mismatch() {
        let left = parse_sexpr("(+ x 1)").expect("parse");
        let right = parse_sexpr("x").expect("parse");

        let mut sample = HashMap::new();
        sample.insert("x".to_string(), r(1, 1));

        assert!(!equiv_on_samples(&left, &right, vec![sample]));
    }
}
